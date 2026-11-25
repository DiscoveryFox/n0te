//! Reading n0te codes off camera pictures ("Mit der Kamera scannen"): camera frames in the
//! formats cameras send (YUYV, BGRA, RGB) become a full-size grey picture for the QR reader
//! (`rqrr`, pure Rust) and a small colour copy for a preview; the text found tells a pair code
//! from an invite.

/// What a scanned text is for n0te.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Code {
    Pair(String),
    Invite(String),
    Other,
}

pub fn classify(text: &str) -> Code {
    let t = text.trim();
    if t.starts_with("n0te-pair:") {
        Code::Pair(t.to_string())
    } else if t.starts_with("n0te://invite/") {
        Code::Invite(t.to_string())
    } else {
        Code::Other
    }
}

/// Preview pictures are at most this wide (the panel shows them at 480 × 360).
const PREVIEW_WIDTH: usize = 480;

/// A camera picture: full-size grey for the QR reader, a small colour copy for the preview.
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub luma: Vec<u8>,
    /// Width, height and RGBA bytes, at most [`PREVIEW_WIDTH`] wide.
    pub preview: (u32, u32, Vec<u8>),
}

impl Frame {
    fn build(width: usize, height: usize, mirror: bool, luma: impl Fn(usize, usize) -> u8, rgb: impl Fn(usize, usize) -> [u8; 3]) -> Option<Frame> {
        if width == 0 || height == 0 {
            return None;
        }
        let mut grey = Vec::with_capacity(width * height);
        for y in 0..height {
            grey.extend((0..width).map(|x| luma(x, y)));
        }
        let pw = width.min(PREVIEW_WIDTH);
        let ph = (height * pw / width).max(1);
        let mut rgba = Vec::with_capacity(pw * ph * 4);
        for py in 0..ph {
            let y = py * height / ph;
            for px in 0..pw {
                let x = px * width / pw;
                let [r, g, b] = rgb(if mirror { width - 1 - x } else { x }, y);
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
        Some(Frame { width, height, luma: grey, preview: (pw as u32, ph as u32, rgba) })
    }

    /// YUYV (YUY2): two pixels in four bytes, Y0 U Y1 V; rows `stride` bytes apart.
    pub fn from_yuyv(data: &[u8], width: usize, height: usize, stride: usize, mirror: bool) -> Option<Frame> {
        let stride = stride.max(width * 2);
        if width == 0 || height == 0 || width % 2 != 0 || data.len() < stride * (height - 1) + width * 2 {
            return None;
        }
        let at = |x: usize, y: usize| y * stride + x * 2;
        Frame::build(
            width,
            height,
            mirror,
            |x, y| data[at(x, y)],
            |x, y| {
                let pair = y * stride + (x & !1) * 2;
                yuv_to_rgb(data[at(x, y)], data[pair + 1], data[pair + 3])
            },
        )
    }

    /// BGRA / BGRX (Windows); rows `stride` bytes apart.
    pub fn from_bgra(data: &[u8], width: usize, height: usize, stride: usize, mirror: bool) -> Option<Frame> {
        let stride = stride.max(width * 4);
        if height == 0 || data.len() < stride * (height - 1) + width * 4 {
            return None;
        }
        let px = |x: usize, y: usize| {
            let i = y * stride + x * 4;
            [data[i + 2], data[i + 1], data[i]]
        };
        Frame::build(width, height, mirror, |x, y| grey(px(x, y)), px)
    }

    /// RGB, three bytes a pixel, rows packed (a decoded MJPEG picture).
    pub fn from_rgb(data: &[u8], width: usize, height: usize, mirror: bool) -> Option<Frame> {
        if data.len() < width * height * 3 {
            return None;
        }
        let px = |x: usize, y: usize| {
            let i = (y * width + x) * 3;
            [data[i], data[i + 1], data[i + 2]]
        };
        Frame::build(width, height, mirror, |x, y| grey(px(x, y)), px)
    }
}

fn grey([r, g, b]: [u8; 3]) -> u8 {
    ((77 * r as u32 + 150 * g as u32 + 29 * b as u32) >> 8) as u8
}

/// BT.601 (studio range, what webcams send) to RGB.
fn yuv_to_rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    let c = 298 * (y as i32 - 16);
    let (d, e) = (u as i32 - 128, v as i32 - 128);
    let clamp = |x: i32| ((x + 128) >> 8).clamp(0, 255) as u8;
    [clamp(c + 409 * e), clamp(c - 100 * d - 208 * e), clamp(c + 516 * d)]
}

/// The text of the first QR code in a grey picture.
pub fn decode(luma: &[u8], width: usize, height: usize) -> Option<String> {
    if width == 0 || height == 0 || luma.len() < width * height {
        return None;
    }
    // Big pictures at half size: quicker, and a code held up to a camera is big anyway.
    let (img, w, h) = if width > 1280 {
        let (w, h) = (width / 2, height / 2);
        let mut half = Vec::with_capacity(w * h);
        for y in 0..h {
            let (top, bottom) = (&luma[2 * y * width..], &luma[(2 * y + 1) * width..]);
            half.extend((0..w).map(|x| ((top[2 * x] as u16 + top[2 * x + 1] as u16 + bottom[2 * x] as u16 + bottom[2 * x + 1] as u16) / 4) as u8));
        }
        (std::borrow::Cow::Owned(half), w, h)
    } else {
        (std::borrow::Cow::Borrowed(luma), width, height)
    };
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| img[y * w + x]);
    prepared.detect_grids().into_iter().find_map(|g| g.decode().ok().map(|(_, text)| text))
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A grey picture `w` × `h` (light grey, like a screen) with `text` as a QR code at
    /// (`x0`, `y0`), `cell` pixels a module, with its quiet zone.
    fn picture(text: &str, w: usize, h: usize, x0: usize, y0: usize, cell: usize) -> Vec<u8> {
        let code = qrcode::QrCode::new(text.as_bytes()).expect("qr");
        let n = code.width();
        let colors = code.to_colors();
        let mut img = vec![200u8; w * h];
        let quiet = 4;
        for my in 0..n + 2 * quiet {
            for mx in 0..n + 2 * quiet {
                let dark = (quiet..n + quiet).contains(&mx) && (quiet..n + quiet).contains(&my) && colors[(my - quiet) * n + mx - quiet] == qrcode::Color::Dark;
                for dy in 0..cell {
                    for dx in 0..cell {
                        let (x, y) = (x0 + mx * cell + dx, y0 + my * cell + dy);
                        if x < w && y < h {
                            img[y * w + x] = if dark { 25 } else { 245 };
                        }
                    }
                }
            }
        }
        img
    }

    const PAIR: &str = "n0te-pair:AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyAhIiMkJSYnKCkqKywtLi8wMTIzNDU2Nzg5Ojs8PT4_QEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9g";
    const INVITE: &str = "n0te://invite/AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0-P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV5fYGFiY2RlZmdoaWprbG1ub3BxcnN0dXZ3eHl6e3x9fn8";

    #[test]
    fn a_frame_with_a_pair_code_yields_the_code() {
        let (w, h) = (640, 480);
        let img = picture(PAIR, w, h, 150, 60, 4);
        assert_eq!(decode(&img, w, h).as_deref(), Some(PAIR));
        assert_eq!(classify(&decode(&img, w, h).unwrap()), Code::Pair(PAIR.to_string()));
        // Nothing in view: nothing found.
        assert_eq!(decode(&vec![200u8; w * h], w, h), None);
    }

    #[test]
    fn big_frames_are_read_at_half_size() {
        let (w, h) = (1920, 1080);
        let img = picture(INVITE, w, h, 700, 200, 6);
        assert_eq!(decode(&img, w, h).as_deref(), Some(INVITE));
    }

    /// The picture as the camera formats bring it: YUYV (Linux webcams), BGRA (Windows), RGB
    /// (decoded MJPEG); the code is read from each, and the preview has the camera's colours.
    #[test]
    fn camera_formats_turn_into_grey_and_preview() {
        let (w, h) = (640, 480);
        let img = picture(PAIR, w, h, 150, 60, 4);
        let stride = w * 2 + 16;
        let mut yuyv = vec![0u8; stride * h];
        for y in 0..h {
            for x in 0..w {
                yuyv[y * stride + x * 2] = img[y * w + x];
                yuyv[y * stride + x * 2 + 1] = 128;
            }
        }
        let bgra: Vec<u8> = img.iter().flat_map(|&g| [g, g, g, 255]).collect();
        let rgb: Vec<u8> = img.iter().flat_map(|&g| [g, g, g]).collect();
        let frames = [Frame::from_yuyv(&yuyv, w, h, stride, false), Frame::from_bgra(&bgra, w, h, w * 4, true), Frame::from_rgb(&rgb, w, h, false)];
        for f in frames {
            let f = f.expect("frame");
            assert_eq!((f.width, f.height), (w, h));
            assert_eq!(decode(&f.luma, f.width, f.height).as_deref(), Some(PAIR));
            let (pw, ph, rgba) = &f.preview;
            assert_eq!((*pw, *ph), (480, 360));
            assert_eq!(rgba.len(), 480 * 360 * 4);
        }
        // Too little data for the size: no frame (rather than a panic).
        assert!(Frame::from_yuyv(&yuyv[..1000], w, h, stride, false).is_none());
        assert!(Frame::from_bgra(&bgra[..1000], w, h, w * 4, false).is_none());
        // Colours: YUYV white and red, BGRA blue.
        assert_eq!(yuv_to_rgb(235, 128, 128), [255, 255, 255]);
        let [r, g, b] = yuv_to_rgb(81, 90, 240);
        assert!(r > 240 && g < 10 && b < 10, "red: {r} {g} {b}");
        let blue = Frame::from_bgra(&[255, 0, 0, 255, 255, 0, 0, 255], 2, 1, 8, false).unwrap();
        assert_eq!(&blue.preview.2[..4], &[0, 0, 255, 255]);
    }

    #[test]
    fn mirrored_preview_flips_left_and_right() {
        // Two pixels: black, white.
        let rgb = [0, 0, 0, 255, 255, 255];
        let plain = Frame::from_rgb(&rgb, 2, 1, false).unwrap();
        let mirrored = Frame::from_rgb(&rgb, 2, 1, true).unwrap();
        assert_eq!(&plain.preview.2[..4], &[0, 0, 0, 255]);
        assert_eq!(&mirrored.preview.2[..4], &[255, 255, 255, 255]);
        // The QR reader gets the picture as it is.
        assert_eq!(plain.luma, mirrored.luma);
    }

    #[test]
    fn codes_are_told_apart() {
        assert_eq!(classify(&format!("  {PAIR}\n")), Code::Pair(PAIR.to_string()));
        assert_eq!(classify(INVITE), Code::Invite(INVITE.to_string()));
        assert_eq!(classify("https://example.com"), Code::Other);
        assert_eq!(classify("n0te://other/x"), Code::Other);
    }
}
