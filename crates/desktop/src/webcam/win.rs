//! Windows cameras through WinRT: `MediaCapture` with a `MediaFrameReader` that hands over BGRA
//! pictures in memory whatever the camera sends (NV12, YUY2, MJPEG: Windows converts). The
//! devices' enclosure tells the front and back cameras of tablets apart. Works in desktop apps
//! without a package; Windows' camera privacy setting ("desktop apps") applies.

use std::time::Duration;

use windows::Devices::Enumeration::{DeviceClass, DeviceInformation, Panel};
use windows::Graphics::Imaging::{BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Capture::Frames::{MediaFrameFormat, MediaFrameReader, MediaFrameReaderAcquisitionMode, MediaFrameReaderStartStatus, MediaFrameReference, MediaFrameSource, MediaFrameSourceKind};
use windows::Media::Capture::{MediaCapture, MediaCaptureInitializationSettings, MediaCaptureMemoryPreference, MediaCaptureSharingMode, MediaStreamType, StreamingCaptureMode};
use windows::Media::MediaProperties::MediaEncodingSubtypes;
use windows::Win32::System::WinRT::IMemoryBufferByteAccess;
use windows::core::{HSTRING, Interface};

use super::{CameraInfo, Facing, Frame};
use crate::i18n::{tr, trf};

/// The camera thread in the multithreaded apartment (WinRT's async calls are waited for on
/// it); left again when dropped.
pub struct Apartment(bool);

pub fn enter_thread() -> Apartment {
    use windows_sys::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    // SAFETY: plain call; balanced by CoUninitialize in drop when it succeeded.
    let hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as _) };
    Apartment(hr >= 0)
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: this thread initialized COM in `enter_thread`.
            unsafe { windows_sys::Win32::System::Com::CoUninitialize() };
        }
    }
}

/// Why a camera can't be used, for people.
fn describe(e: windows::core::Error) -> String {
    match e.code().0 as u32 {
        // E_ACCESSDENIED: the privacy setting.
        0x8007_0005 => tr("Windows lässt n0te nicht an die Kamera: Einstellungen → Datenschutz und Sicherheit → Kamera → Desktop-Apps den Zugriff erlauben").to_string(),
        // MF_E_HW_MFT_FAILED_START_STREAMING, MF_E_VIDEO_RECORDING_DEVICE_LOCKED / _PREEMPTED, ERROR_BUSY.
        0xC00D_3704 | 0xC00D_3E80 | 0xC00D_3E81 | 0x8007_00AA => tr("Die Kamera wird gerade von einer anderen App benutzt").to_string(),
        code => {
            let what = e.message();
            trf("Kamera: {}", &[&if what.trim().is_empty() { format!("0x{code:08X}") } else { format!("{} (0x{code:08X})", what.trim()) }])
        }
    }
}

/// The cameras (built in and plugged in), as Windows lists them.
pub fn cameras() -> Result<Vec<CameraInfo>, String> {
    let all = DeviceInformation::FindAllAsyncDeviceClass(DeviceClass::VideoCapture).and_then(|op| op.join()).map_err(describe)?;
    let mut out = Vec::new();
    for i in 0..all.Size().map_err(describe)? {
        let Ok(d) = all.GetAt(i) else { continue };
        if !d.IsEnabled().unwrap_or(true) {
            continue;
        }
        let (Ok(id), Ok(name)) = (d.Id(), d.Name()) else { continue };
        let facing = match d.EnclosureLocation().and_then(|l| l.Panel()) {
            Ok(p) if p == Panel::Front => Facing::Front,
            Ok(p) if p == Panel::Back => Facing::Back,
            _ => Facing::Unknown,
        };
        out.push(CameraInfo { id: id.to_string(), name: name.to_string(), facing });
    }
    Ok(out)
}

/// An open camera with a running frame reader; dropping it stops both and lets go.
pub struct Camera {
    capture: MediaCapture,
    reader: MediaFrameReader,
}

fn initialize(id: &str, sharing: MediaCaptureSharingMode) -> windows::core::Result<MediaCapture> {
    let settings = MediaCaptureInitializationSettings::new()?;
    settings.SetVideoDeviceId(&HSTRING::from(id))?;
    settings.SetStreamingCaptureMode(StreamingCaptureMode::Video)?;
    // Pictures in memory (a SoftwareBitmap), not on the GPU.
    settings.SetMemoryPreference(MediaCaptureMemoryPreference::Cpu)?;
    settings.SetSharingMode(sharing)?;
    let capture = MediaCapture::new()?;
    capture.InitializeWithSettingsAsync(&settings)?.join()?;
    Ok(capture)
}

/// The colour stream to read: the preview one if there is one, else the recording one.
fn color_source(capture: &MediaCapture) -> Option<MediaFrameSource> {
    let mut best: Option<(u8, MediaFrameSource)> = None;
    for pair in capture.FrameSources().ok()?.First().ok()? {
        let Ok(source) = pair.Value() else { continue };
        let Ok(info) = source.Info() else { continue };
        if info.SourceKind().ok() != Some(MediaFrameSourceKind::Color) {
            continue;
        }
        let rank = match info.MediaStreamType() {
            Ok(t) if t == MediaStreamType::VideoPreview => 0,
            Ok(t) if t == MediaStreamType::VideoRecord => 1,
            _ => 2,
        };
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, source));
        }
    }
    best.map(|(_, s)| s)
}

/// The format nearest 1280 × 720 with at least 10 pictures a second (tablet cameras offer up to
/// 8 megapixels: far more than a QR code needs, and slow).
fn format_near_hd(source: &MediaFrameSource) -> Option<MediaFrameFormat> {
    let formats = source.SupportedFormats().ok()?;
    let mut best: Option<(i64, MediaFrameFormat)> = None;
    for i in 0..formats.Size().ok()? {
        let Ok(f) = formats.GetAt(i) else { continue };
        let Ok(v) = f.VideoFormat() else { continue };
        let (Ok(w), Ok(h)) = (v.Width(), v.Height()) else { continue };
        let fps = f.FrameRate().ok().map(|r| r.Numerator().unwrap_or(0) as f32 / r.Denominator().unwrap_or(1).max(1) as f32).unwrap_or(30.0);
        if fps < 10.0 {
            continue;
        }
        let distance = (w as i64 * h as i64 - 1280 * 720).abs();
        if best.as_ref().is_none_or(|(d, _)| distance < *d) {
            best = Some((distance, f));
        }
    }
    best.map(|(_, f)| f)
}

/// Why opening failed: Windows' privacy setting says no (trying again won't help), or another
/// reason (for people).
enum OpenError {
    Denied(String),
    Other(String),
}

impl From<windows::core::Error> for OpenError {
    fn from(e: windows::core::Error) -> Self {
        if e.code().0 as u32 == 0x8007_0005 { OpenError::Denied(describe(e)) } else { OpenError::Other(describe(e)) }
    }
}

fn open_with(id: &str, sharing: MediaCaptureSharingMode) -> Result<Camera, OpenError> {
    let capture = initialize(id, sharing)?;
    let opened = (|| {
        let source = color_source(&capture).ok_or_else(|| OpenError::Other(tr("Die Kamera liefert keine Farbbilder").to_string()))?;
        if let Some(f) = format_near_hd(&source) {
            // Read-only sharing can't change it: the camera's own format then.
            let _ = source.SetFormatAsync(&f).and_then(|a| a.join());
        }
        let bgra = MediaEncodingSubtypes::Bgra8()?;
        let reader = capture.CreateFrameReaderWithSubtypeAsync(&source, &bgra)?.join()?;
        let _ = reader.SetAcquisitionMode(MediaFrameReaderAcquisitionMode::Realtime);
        let status = reader.StartAsync()?.join()?;
        if status == MediaFrameReaderStartStatus::Success {
            Ok(reader)
        } else if status == MediaFrameReaderStartStatus::DeviceNotAvailable || status == MediaFrameReaderStartStatus::ExclusiveControlNotAvailable {
            let _ = reader.Close();
            Err(OpenError::Other(tr("Die Kamera wird gerade von einer anderen App benutzt").to_string()))
        } else {
            let _ = reader.Close();
            Err(OpenError::Other(trf("Kamera: {}", &[&format!("{status:?}")])))
        }
    })();
    match opened {
        Ok(reader) => Ok(Camera { capture, reader }),
        Err(e) => {
            let _ = capture.Close();
            Err(e)
        }
    }
}

impl Camera {
    pub fn open(info: &CameraInfo) -> Result<Camera, String> {
        // Exclusive control lets us pick the format; while another app uses the camera it may
        // still be shared read-only (in the format that app chose).
        match open_with(&info.id, MediaCaptureSharingMode::ExclusiveControl) {
            Ok(c) => Ok(c),
            Err(OpenError::Denied(why)) => Err(why),
            Err(OpenError::Other(first)) => match open_with(&info.id, MediaCaptureSharingMode::SharedReadOnly) {
                Ok(c) => Ok(c),
                Err(OpenError::Denied(why)) => Err(why),
                Err(OpenError::Other(_)) => Err(first),
            },
        }
    }

    /// The newest picture (None: nothing new yet).
    pub fn frame(&mut self, mirror: bool) -> Result<Option<Frame>, String> {
        let Ok(reference) = self.reader.TryAcquireLatestFrame() else {
            std::thread::sleep(Duration::from_millis(15));
            return Ok(None);
        };
        let frame = bitmap_frame(&reference, mirror);
        let _ = reference.Close();
        Ok(frame)
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        let _ = self.reader.StopAsync().and_then(|a| a.join());
        let _ = self.reader.Close();
        let _ = self.capture.Close();
    }
}

/// The picture of a frame as BGRA bytes (converted when the camera sent something else).
fn bitmap_frame(reference: &MediaFrameReference, mirror: bool) -> Option<Frame> {
    let bitmap = reference.VideoMediaFrame().ok()?.SoftwareBitmap().ok()?;
    let bitmap = if bitmap.BitmapPixelFormat().ok()? == BitmapPixelFormat::Bgra8 { bitmap } else { SoftwareBitmap::Convert(&bitmap, BitmapPixelFormat::Bgra8).ok()? };
    let (w, h) = (bitmap.PixelWidth().ok()? as usize, bitmap.PixelHeight().ok()? as usize);
    let buffer = bitmap.LockBuffer(BitmapBufferAccessMode::Read).ok()?;
    let plane = buffer.GetPlaneDescription(0).ok()?;
    let memory = buffer.CreateReference().ok()?;
    let access: IMemoryBufferByteAccess = memory.cast().ok()?;
    let mut data = std::ptr::null_mut();
    let mut len = 0u32;
    // SAFETY: out pointers to locals; the bytes stay valid while `memory` is open (until the
    // Close below, after the copy).
    let frame = match unsafe { access.GetBuffer(&mut data, &mut len) } {
        Ok(()) if !data.is_null() => {
            // SAFETY: `len` bytes at `data`, owned by the locked buffer.
            let bytes = unsafe { std::slice::from_raw_parts(data, len as usize) };
            bytes.get(plane.StartIndex.max(0) as usize..).and_then(|b| Frame::from_bgra(b, w, h, plane.Stride.max(0) as usize, mirror))
        }
        _ => None,
    };
    let _ = memory.Close();
    let _ = buffer.Close();
    let _ = bitmap.Close();
    frame
}
