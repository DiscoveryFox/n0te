//! "Mit der Kamera scannen": a small live camera picture over everything that reads QR codes as
//! they come into view. A pair code (`n0te-pair:…`) pairs like a pasted one, an invite
//! (`n0te://invite/…`) fills the join dialog, any other code gets a short hint. Windows
//! ([`win`]: WinRT `MediaCapture`, the front and back cameras of pen tablets) and Linux
//! ([`linux`]: V4L2); elsewhere [`supported`] is false and no button shows.
//!
//! The camera is opened, read and decoded on a thread of its own that [`subscription`] starts
//! while a [`Scanner`] is open: the UI only gets small preview pictures (at most 12 a second;
//! one that finds the UI busy is dropped) and the codes found. Closing the panel drops the
//! subscription's stream, which stops the thread and lets go of the camera. Tests never open a
//! camera: the test harness runs no subscriptions, and the decoding is tested on made-up frames.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iced::futures::StreamExt;
use iced::futures::channel::mpsc;
use iced::widget::image::Handle;
use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Color, ContentFit, Element, Length, Padding, Subscription};

use crate::i18n::{tr, trf};
use crate::theme;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(windows)]
mod win;
#[cfg(windows)]
use win as platform;

/// Whether this platform can scan with a camera (else the buttons don't show).
pub fn supported() -> bool {
    cfg!(any(windows, target_os = "linux"))
}

/// Where the scanner was opened: it says what to hold up (either code works in both).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Pair,
    Join,
}

/// Which way a camera looks (tablets say; webcams don't).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facing {
    Front,
    Back,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraInfo {
    /// What opens it again: the device path (Linux), the device id (Windows).
    pub id: String,
    pub name: String,
    pub facing: Facing,
}

impl CameraInfo {
    /// Its button: "Hinten · Microsoft Camera Rear".
    pub fn label(&self) -> String {
        match self.facing {
            Facing::Front => format!("{} · {}", tr("Vorne"), self.name),
            Facing::Back => format!("{} · {}", tr("Hinten"), self.name),
            Facing::Unknown => self.name.clone(),
        }
    }
}

/// What the camera thread reports.
#[derive(Debug, Clone)]
pub enum Event {
    /// The cameras there are, and the one being opened.
    Cameras(Vec<CameraInfo>, Option<usize>),
    /// A preview picture.
    Frame(Handle),
    /// The text of a QR code in view.
    Found(String),
    /// The camera can't be used (why); the thread has ended.
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum Msg {
    Open(Purpose),
    Close,
    /// Another camera (its id).
    Pick(String),
    Retry,
    /// From the camera thread of a session; a closed or replaced session's are ignored.
    Event(u64, Event),
}

/// What the app does after a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// Pair with this code (like `PairAccept`).
    Pair(String),
    /// Put this link into the join dialog.
    Invite(String),
}

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

/// The open camera panel.
#[derive(Debug)]
pub struct Scanner {
    pub purpose: Purpose,
    /// Its camera thread (a new one for another camera or a retry).
    session: u64,
    /// The camera asked for (None: the last one used, else a back camera, else the first).
    want: Option<String>,
    pub cameras: Vec<CameraInfo>,
    pub current: Option<usize>,
    pub preview: Option<Handle>,
    pub failed: Option<String>,
    /// When a QR code that isn't n0te's was last seen (the hint shows a moment).
    pub other_at: Option<Instant>,
}

static SESSIONS: AtomicU64 = AtomicU64::new(1);

impl Scanner {
    pub fn new(purpose: Purpose) -> Self {
        Scanner { purpose, session: SESSIONS.fetch_add(1, Ordering::Relaxed), want: None, cameras: Vec::new(), current: None, preview: None, failed: None, other_at: None }
    }

    /// Starts over with a new camera thread (the old one stops), keeping the camera list.
    fn restart(&mut self, want: Option<String>) {
        self.session = SESSIONS.fetch_add(1, Ordering::Relaxed);
        if let Some(id) = &want {
            self.current = self.cameras.iter().position(|c| &c.id == id);
        }
        self.want = want;
        self.preview = None;
        self.failed = None;
        self.other_at = None;
    }

    fn event(&mut self, e: Event) -> Outcome {
        match e {
            Event::Cameras(list, current) => {
                self.cameras = list;
                self.current = current;
            }
            Event::Frame(h) => self.preview = Some(h),
            Event::Found(t) => match classify(&t) {
                Code::Pair(c) => return Outcome::Pair(c),
                Code::Invite(l) => return Outcome::Invite(l),
                Code::Other => self.other_at = Some(Instant::now()),
            },
            Event::Failed(why) => {
                self.failed = Some(why);
                self.preview = None;
            }
        }
        Outcome::Nothing
    }

    /// The camera thread its events must come from (see [`Msg::Event`]).
    pub fn session(&self) -> u64 {
        self.session
    }

    pub fn other_shown(&self) -> bool {
        self.other_at.is_some_and(|t| t.elapsed() < Duration::from_secs(3))
    }
}

/// Runs a message on the panel (`scan`: None when closed). A code found closes it.
pub fn update(scan: &mut Option<Scanner>, msg: Msg) -> Outcome {
    let outcome = match msg {
        Msg::Open(purpose) => {
            *scan = Some(Scanner::new(purpose));
            Outcome::Nothing
        }
        Msg::Close => {
            *scan = None;
            Outcome::Nothing
        }
        msg => {
            let Some(s) = scan.as_mut() else { return Outcome::Nothing };
            match msg {
                Msg::Pick(id) => {
                    if s.failed.is_some() || s.current.and_then(|i| s.cameras.get(i)).is_none_or(|c| c.id != id) {
                        s.restart(Some(id));
                    }
                    Outcome::Nothing
                }
                Msg::Retry => {
                    let want = s.want.clone();
                    s.restart(want);
                    Outcome::Nothing
                }
                Msg::Event(session, e) if session == s.session => s.event(e),
                Msg::Event(..) | Msg::Open(_) | Msg::Close => Outcome::Nothing,
            }
        }
    };
    if outcome != Outcome::Nothing {
        *scan = None;
    }
    outcome
}

/// The camera thread while the panel is open (and hasn't failed).
pub fn subscription(scan: Option<&Scanner>) -> Subscription<Msg> {
    match scan {
        Some(s) if supported() && s.failed.is_none() => Subscription::run_with((s.session, s.want.clone()), stream),
        _ => Subscription::none(),
    }
}

/// Sets a flag when dropped: the stream it lives in is gone, the camera thread stops.
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn stream(key: &(u64, Option<String>)) -> impl iced::futures::Stream<Item = Msg> + use<> {
    let (session, want) = key.clone();
    // Room for a couple of events: a preview that finds it full is skipped.
    let (tx, rx) = mpsc::channel(2);
    let stop = Arc::new(AtomicBool::new(false));
    let guard = StopOnDrop(stop.clone());
    let mut failed = tx.clone();
    if let Err(e) = std::thread::Builder::new().name("n0te-camera".into()).spawn(move || capture(want, tx, &stop)) {
        let _ = failed.try_send(Event::Failed(trf("Kamera: {}", &[&e])));
    }
    rx.map(move |e| {
        let _ = &guard;
        Msg::Event(session, e)
    })
}

/// One camera thread at a time: a new one waits until the last has let go of its camera.
static DEVICE: Mutex<()> = Mutex::new(());
/// The camera last picked by hand (this run), opened first next time.
static LAST: Mutex<Option<String>> = Mutex::new(None);
/// Turns of the camera loop at most this often (≤ 12.5 pictures a second).
const TURN: Duration = Duration::from_millis(80);
/// Preview pictures are at most this wide (the panel shows them at 480 × 360).
const PREVIEW_WIDTH: usize = 480;

fn capture(want: Option<String>, mut tx: mpsc::Sender<Event>, stop: &AtomicBool) {
    let _one = DEVICE.lock().unwrap_or_else(|e| e.into_inner());
    if stop.load(Ordering::Relaxed) {
        return;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(want, &mut tx, stop))).unwrap_or_else(|_| Err(tr("Kamera: interner Fehler").to_string()));
    if let Err(why) = result {
        send(&mut tx, Event::Failed(why), stop);
    }
}

/// Sends an event that must arrive (waits while the channel is full); false once nobody listens.
fn send(tx: &mut mpsc::Sender<Event>, mut e: Event, stop: &AtomicBool) -> bool {
    loop {
        match tx.try_send(e) {
            Ok(()) => return true,
            Err(err) if err.is_full() && !stop.load(Ordering::Relaxed) => {
                e = err.into_inner();
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return false,
        }
    }
}

/// The camera to open: the one asked for, the last one picked, a back camera, the first.
fn choose(cameras: &[CameraInfo], want: Option<&str>, last: Option<&str>) -> Option<usize> {
    let find = |id: &str| cameras.iter().position(|c| c.id == id);
    want.and_then(find).or_else(|| last.and_then(find)).or_else(|| cameras.iter().position(|c| c.facing == Facing::Back)).or_else(|| (!cameras.is_empty()).then_some(0))
}

#[cfg(any(windows, target_os = "linux"))]
fn run(want: Option<String>, tx: &mut mpsc::Sender<Event>, stop: &AtomicBool) -> Result<(), String> {
    let _thread = platform::enter_thread();
    let cameras = platform::cameras()?;
    let last = LAST.lock().ok().and_then(|l| l.clone());
    let pick = choose(&cameras, want.as_deref(), last.as_deref());
    if !send(tx, Event::Cameras(cameras.clone(), pick), stop) {
        return Ok(());
    }
    let Some(info) = pick.and_then(|i| cameras.get(i)) else { return Err(tr("Keine Kamera gefunden").to_string()) };
    // The last session may still be letting go of this camera: a few tries.
    let mut tries = 0;
    let mut camera = loop {
        match platform::Camera::open(info) {
            Ok(c) => break c,
            Err(e) if tries >= 3 || stop.load(Ordering::Relaxed) => return Err(e),
            Err(_) => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(400));
            }
        }
    };
    if want.is_some()
        && let Ok(mut l) = LAST.lock()
    {
        *l = Some(info.id.clone());
    }
    // A camera looking at the user shows like a mirror (easier to aim); the codes are read
    // from the picture as it is.
    let mirror = info.facing != Facing::Back;
    let mut last_found: Option<(String, Instant)> = None;
    let mut last_frame = Instant::now();
    let mut next = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        if now < next {
            std::thread::sleep(next - now);
        }
        next = Instant::now() + TURN;
        let Some(frame) = camera.frame(mirror)? else {
            if last_frame.elapsed() > Duration::from_secs(8) {
                return Err(tr("Die Kamera liefert keine Bilder").to_string());
            }
            continue;
        };
        last_frame = Instant::now();
        if let Some(found) = decode(&frame.luma, frame.width, frame.height) {
            // The same code again and again: reported once every two seconds.
            let again = last_found.as_ref().is_some_and(|(t, at)| *t == found && at.elapsed() < Duration::from_secs(2));
            if !again {
                last_found = Some((found.clone(), Instant::now()));
                if !send(tx, Event::Found(found), stop) {
                    return Ok(());
                }
            }
        }
        let (w, h, rgba) = frame.preview;
        if let Err(e) = tx.try_send(Event::Frame(Handle::from_rgba(w, h, rgba)))
            && e.is_disconnected()
        {
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn run(_: Option<String>, _: &mut mpsc::Sender<Event>, _: &AtomicBool) -> Result<(), String> {
    Err(tr("Keine Kamera gefunden").to_string())
}

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

const PREVIEW_W: f32 = 480.0;
const PREVIEW_H: f32 = 360.0;

/// The panel: what to hold up, the cameras to choose from, the live picture, a hint.
pub fn view(s: &Scanner) -> Element<'_, Msg> {
    let p = theme::p();
    let intro = match s.purpose {
        Purpose::Pair => tr("Halte den Koppel-Code des anderen Geräts vor die Kamera."),
        Purpose::Join => tr("Halte den QR-Code der Einladung vor die Kamera."),
    };
    let mut col = column![
        row![
            theme::heading(tr("Mit der Kamera scannen"), 20.0),
            Space::new().width(Length::Fill),
            button(theme::btn(tr("Abbrechen"))).padding(theme::pad()).on_press(Msg::Close).style(theme::ghost),
        ]
        .align_y(Alignment::Center),
        text(intro).size(13).color(p.muted),
    ]
    .spacing(10);
    if s.cameras.len() > 1 {
        let mut pick = row![].spacing(6);
        for (i, c) in s.cameras.iter().enumerate() {
            pick = pick.push(button(theme::btn_small(&c.label())).padding(theme::pad()).on_press(Msg::Pick(c.id.clone())).style(theme::tab(s.current == Some(i))));
        }
        col = col.push(pick.wrap());
    }
    let light = Color::from_rgb(0.85, 0.85, 0.85);
    let picture: Element<'_, Msg> = match (&s.failed, &s.preview) {
        (Some(why), _) => text(why.clone()).size(13).color(light).center().into(),
        (None, Some(h)) => iced::widget::image(h.clone()).width(Length::Fill).height(Length::Fill).content_fit(ContentFit::Contain).into(),
        (None, None) => text(if s.cameras.is_empty() { tr("Suche Kameras …") } else { tr("Kamera startet …") }).size(13).color(light).into(),
    };
    col = col.push(
        container(picture)
            .padding(12)
            .width(Length::Fixed(PREVIEW_W))
            .height(Length::Fixed(PREVIEW_H))
            .center_x(Length::Fixed(PREVIEW_W))
            .center_y(Length::Fixed(PREVIEW_H))
            .style(|_| container::Style { background: Some(Color::BLACK.into()), border: iced::Border { radius: 6.0.into(), ..Default::default() }, ..Default::default() }),
    );
    if s.other_shown() {
        col = col.push(text(tr("Das ist kein n0te-Code")).size(13).color(p.accent));
    }
    if s.failed.is_some() {
        col = col.push(button(theme::btn(tr("Nochmal versuchen"))).padding(theme::pad()).on_press(Msg::Retry).style(theme::secondary));
    }
    container(container(col).padding(20).width(Length::Fixed(PREVIEW_W + 40.0)).style(theme::panel)).padding(Padding { top: 70.0, ..Padding::ZERO }).center_x(Length::Fill).into()
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

    #[test]
    fn the_camera_asked_for_then_the_last_then_a_back_one() {
        let cam = |id: &str, facing| CameraInfo { id: id.into(), name: id.into(), facing };
        let cams = [cam("front", Facing::Front), cam("back", Facing::Back), cam("usb", Facing::Unknown)];
        assert_eq!(choose(&cams, Some("usb"), Some("front")), Some(2));
        assert_eq!(choose(&cams, None, Some("front")), Some(0));
        assert_eq!(choose(&cams, Some("gone"), None), Some(1));
        assert_eq!(choose(&cams[2..], None, None), Some(0));
        assert_eq!(choose(&[], None, None), None);
    }

    #[test]
    fn found_codes_close_the_panel_and_say_what_to_do() {
        let mut scan = None;
        assert_eq!(update(&mut scan, Msg::Open(Purpose::Pair)), Outcome::Nothing);
        let session = scan.as_ref().unwrap().session;
        // Another QR code: a hint, the panel stays.
        assert_eq!(update(&mut scan, Msg::Event(session, Event::Found("hello".into()))), Outcome::Nothing);
        assert!(scan.as_ref().unwrap().other_shown());
        // A replaced session's events don't count.
        assert_eq!(update(&mut scan, Msg::Event(session + 1000, Event::Found(PAIR.into()))), Outcome::Nothing);
        assert!(scan.is_some());
        assert_eq!(update(&mut scan, Msg::Event(session, Event::Found(PAIR.into()))), Outcome::Pair(PAIR.into()));
        assert!(scan.is_none(), "closed (the camera thread stops)");

        update(&mut scan, Msg::Open(Purpose::Join));
        let s = scan.as_ref().unwrap().session;
        let cams = vec![CameraInfo { id: "a".into(), name: "A".into(), facing: Facing::Front }, CameraInfo { id: "b".into(), name: "B".into(), facing: Facing::Back }];
        update(&mut scan, Msg::Event(s, Event::Cameras(cams, Some(1))));
        // Another camera: a new session (the old thread's events are ignored from now on).
        update(&mut scan, Msg::Pick("a".into()));
        let open = scan.as_ref().unwrap();
        assert_ne!(open.session, s);
        assert_eq!(open.current, Some(0));
        let s = open.session;
        update(&mut scan, Msg::Event(s, Event::Failed("busy".into())));
        assert_eq!(scan.as_ref().unwrap().failed.as_deref(), Some("busy"));
        update(&mut scan, Msg::Retry);
        let open = scan.as_ref().unwrap();
        assert!(open.failed.is_none());
        assert_eq!(open.want.as_deref(), Some("a"), "the retry opens the same camera");
        let s = open.session;
        assert_eq!(update(&mut scan, Msg::Event(s, Event::Found(INVITE.into()))), Outcome::Invite(INVITE.into()));
        assert!(scan.is_none());
        update(&mut scan, Msg::Open(Purpose::Join));
        update(&mut scan, Msg::Close);
        assert!(scan.is_none());
    }
}
