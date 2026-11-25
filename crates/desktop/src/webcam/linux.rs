//! Linux cameras through V4L2 (`linuxvideo`: plain ioctls, no C library): YUYV at 640 × 480,
//! which every UVC webcam has, else MJPEG. Nodes without colour video (metadata, IR cameras
//! that only send grey) aren't offered.

use std::io;
use std::os::fd::AsRawFd;
use std::path::PathBuf;

use linuxvideo::format::{FrameSizes, PixFormat, PixelFormat};
use linuxvideo::stream::ReadStream;
use linuxvideo::{BufType, CapabilityFlags, Device};

use super::{CameraInfo, Facing, Frame};
use crate::i18n::{tr, trf};

/// Nothing to set up on a camera thread here (Windows joins COM's apartment).
pub struct Thread;

pub fn enter_thread() -> Thread {
    Thread
}

/// The cameras, /dev/video0 first. Opening a node to ask doesn't switch the camera on.
pub fn cameras() -> Result<Vec<CameraInfo>, String> {
    let mut nodes: Vec<(u32, PathBuf)> = std::fs::read_dir("/dev")
        .map_err(|e| trf("Kamera: {}", &[&e]))?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_str()?.strip_prefix("video")?.parse().ok()?;
            Some((n, e.path()))
        })
        .collect();
    nodes.sort();
    let mut out = Vec::new();
    for (_, path) in nodes {
        let Ok(dev) = Device::open(&path) else { continue };
        let Ok(caps) = dev.capabilities() else { continue };
        if !caps.device_capabilities().contains(CapabilityFlags::VIDEO_CAPTURE | CapabilityFlags::STREAMING) || pick_format(&dev).is_none() {
            continue;
        }
        out.push(CameraInfo { id: path.display().to_string(), name: caps.card().trim().to_string(), facing: Facing::Unknown });
    }
    Ok(out)
}

/// YUYV near 640 × 480 (smooth even on USB 2), else MJPEG near 1280 × 720.
fn pick_format(dev: &Device) -> Option<(PixelFormat, u32, u32)> {
    let formats: Vec<PixelFormat> = dev.formats(BufType::VIDEO_CAPTURE).flatten().map(|f| f.pixel_format()).collect();
    let (format, (w, h)) = [(PixelFormat::YUYV, (640, 480)), (PixelFormat::MJPG, (1280, 720))].into_iter().find(|(f, _)| formats.contains(f))?;
    let size = match dev.frame_sizes(format) {
        Ok(FrameSizes::Discrete(sizes)) => sizes.iter().map(|s| (s.width(), s.height())).min_by_key(|&(sw, sh)| (sw as i64 * sh as i64 - w as i64 * h as i64).abs()).unwrap_or((w, h)),
        Ok(FrameSizes::Stepwise(s) | FrameSizes::Continuous(s)) => (w.max(s.min_width()).min(s.max_width()), h.max(s.min_height()).min(s.max_height())),
        Err(_) => (w, h),
    };
    Some((format, size.0, size.1))
}

/// Why a camera can't be used, for people.
fn describe(e: io::Error) -> String {
    match e.raw_os_error() {
        Some(libc::EBUSY) => tr("Die Kamera wird gerade von einer anderen App benutzt").to_string(),
        Some(libc::EACCES | libc::EPERM) => tr("Kein Zugriff auf die Kamera (Gruppe „video“?)").to_string(),
        Some(libc::ENODEV | libc::ENOENT) => tr("Die Kamera ist nicht mehr da").to_string(),
        _ => trf("Kamera: {}", &[&e]),
    }
}

/// An open, streaming camera; dropping it stops the stream and closes the device.
pub struct Camera {
    stream: ReadStream,
    mjpeg: bool,
    width: usize,
    height: usize,
    stride: usize,
    /// The newest picture's bytes (copied out of the driver's buffer).
    raw: Vec<u8>,
}

impl Camera {
    pub fn open(info: &CameraInfo) -> Result<Camera, String> {
        let dev = Device::open(&info.id).map_err(describe)?;
        let (format, w, h) = pick_format(&dev).ok_or_else(|| tr("Die Kamera liefert kein lesbares Bildformat").to_string())?;
        let capture = dev.video_capture(PixFormat::new(w, h, format)).map_err(describe)?;
        // What the driver agreed to.
        let got = capture.format();
        let (format, width, height, stride) = (got.pixel_format(), got.width() as usize, got.height() as usize, got.bytes_per_line() as usize);
        if format != PixelFormat::YUYV && format != PixelFormat::MJPG {
            return Err(tr("Die Kamera liefert kein lesbares Bildformat").to_string());
        }
        let stream = capture.into_stream().map_err(describe)?;
        Ok(Camera { stream, mjpeg: format == PixelFormat::MJPG, width, height, stride: stride.max(width * 2), raw: Vec::new() })
    }

    /// The newest picture; None when none came within a quarter second (so a stop is noticed).
    pub fn frame(&mut self, mirror: bool) -> Result<Option<Frame>, String> {
        let mut fd = libc::pollfd { fd: self.stream.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: one valid pollfd for the stream's open file.
        let ready = unsafe { libc::poll(&mut fd, 1, 250) };
        if ready < 0 {
            let e = io::Error::last_os_error();
            return if e.kind() == io::ErrorKind::Interrupted { Ok(None) } else { Err(describe(e)) };
        }
        if ready == 0 {
            return Ok(None);
        }
        // Older pictures still queued are skipped: only the last one is used.
        let mut good = false;
        for _ in 0..8 {
            let raw = &mut self.raw;
            good = self
                .stream
                .dequeue(|buf| {
                    raw.clear();
                    raw.extend_from_slice(&buf);
                    Ok(!buf.is_error())
                })
                .map_err(describe)?;
            if self.stream.will_block().unwrap_or(true) {
                break;
            }
        }
        if !good {
            return Ok(None);
        }
        Ok(if self.mjpeg { jpeg_frame(&self.raw, mirror) } else { Frame::from_yuyv(&self.raw, self.width, self.height, self.stride, mirror) })
    }
}

fn jpeg_frame(data: &[u8], mirror: bool) -> Option<Frame> {
    let img = image::load_from_memory_with_format(&with_huffman_tables(data), image::ImageFormat::Jpeg).ok()?.into_rgb8();
    Frame::from_rgb(img.as_raw(), img.width() as usize, img.height() as usize, mirror)
}

/// UVC webcams leave the Huffman tables out of their MJPEG pictures (JPEG's standard ones,
/// annex K.3, are meant); the decoder needs them written in, before the scan.
fn with_huffman_tables(jpeg: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    use std::borrow::Cow;
    if !jpeg.starts_with(&[0xFF, 0xD8]) {
        return Cow::Borrowed(jpeg);
    }
    let mut i = 2;
    while i + 4 <= jpeg.len() && jpeg[i] == 0xFF {
        match jpeg[i + 1] {
            0xC4 => return Cow::Borrowed(jpeg),
            0xDA => {
                let mut out = Vec::with_capacity(jpeg.len() + 420);
                out.extend_from_slice(&jpeg[..i]);
                let len = 2 + HUFFMAN.iter().map(|(_, _, values)| 17 + values.len()).sum::<usize>();
                out.extend_from_slice(&[0xFF, 0xC4, (len >> 8) as u8, len as u8]);
                for (id, lengths, values) in HUFFMAN {
                    out.push(id);
                    out.extend_from_slice(lengths);
                    out.extend_from_slice(values);
                }
                out.extend_from_slice(&jpeg[i..]);
                return Cow::Owned(out);
            }
            // Fill byte; markers without a length.
            0xFF => i += 1,
            0x01 | 0xD0..=0xD7 => i += 2,
            _ => i += 2 + u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize,
        }
    }
    Cow::Borrowed(jpeg)
}

/// JPEG's standard Huffman tables (annex K.3): class/id, code lengths, values.
const HUFFMAN: [(u8, &[u8; 16], &[u8]); 4] = [(0x00, &LUMA_DC_LENGTHS, &DC_VALUES), (0x01, &CHROMA_DC_LENGTHS, &DC_VALUES), (0x10, &LUMA_AC_LENGTHS, &LUMA_AC_VALUES), (0x11, &CHROMA_AC_LENGTHS, &CHROMA_AC_VALUES)];
const LUMA_DC_LENGTHS: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const CHROMA_DC_LENGTHS: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const DC_VALUES: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const LUMA_AC_LENGTHS: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7D];
#[rustfmt::skip]
const LUMA_AC_VALUES: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
    0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0,
    0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7,
    0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5,
    0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
    0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];
const CHROMA_AC_LENGTHS: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
#[rustfmt::skip]
const CHROMA_AC_VALUES: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
    0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xA1, 0xB1, 0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0,
    0x15, 0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34, 0xE1, 0x25, 0xF1, 0x17, 0x18, 0x19, 0x1A, 0x26,
    0x27, 0x28, 0x29, 0x2A, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
    0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
    0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3,
    0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA,
    0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// UVC webcams send MJPEG pictures without their Huffman tables (the standard ones are
    /// meant); the decoder must cope.
    #[test]
    fn mjpeg_without_huffman_tables_decodes() {
        let (w, h) = (64u32, 48u32);
        let rgb: Vec<u8> = (0..w * h).flat_map(|i| if (i % w) < w / 2 { [20, 20, 20] } else { [230, 230, 230] }).collect();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode(&rgb, w, h, image::ExtendedColorType::Rgb8).unwrap();
        let frame = jpeg_frame(&jpeg, false).expect("a plain JPEG");
        assert_eq!((frame.width, frame.height), (64, 48));
        // Cut out the DHT segments (marker FF C4, then a big-endian length).
        let mut cut = Vec::new();
        let mut i = 0;
        while i < jpeg.len() {
            if jpeg[i] == 0xFF && jpeg.get(i + 1) == Some(&0xC4) {
                let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
                i += 2 + len;
                continue;
            }
            if jpeg[i] == 0xFF && jpeg.get(i + 1) == Some(&0xDA) {
                cut.extend_from_slice(&jpeg[i..]);
                break;
            }
            cut.push(jpeg[i]);
            i += 1;
        }
        assert!(cut.len() < jpeg.len(), "the tables were cut");
        let frame = jpeg_frame(&cut, false).expect("MJPEG without tables");
        assert!(frame.luma[0] < 80 && frame.luma[w as usize - 1] > 180, "dark left, light right");
    }
}
