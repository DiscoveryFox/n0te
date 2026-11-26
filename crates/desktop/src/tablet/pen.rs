//! Pen input for n0te Tablet.
//!
//! winit (under iced) turns a pen into plain touches: no pressure, tilt, eraser end, barrel
//! button or hover. On Windows we read the pen ourselves: the window is subclassed and the
//! `WM_POINTER*` messages of pen pointers that go down on the canvas (see [`set_ink_areas`]) are
//! taken before winit sees them; everything else (pen taps on buttons, fingers, the mouse) goes
//! on to winit as before. Positions come from the digitizer's HIMETRIC coordinates mapped onto
//! the screen, so they are finer than a pixel, and every coalesced sample of a frame is kept
//! (`GetPointerPenInfoHistory`). Windows' own pen gestures on the window (press-and-hold right
//! click, flicks, tap feedback) are switched off.
//!
//! Elsewhere (Linux; tests) there is no pen backend: the page view turns a mouse drag into the
//! same [`PenEvent`]s, so drawing works and can be tested there too.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::futures::channel::mpsc;
use iced::{Rectangle, Subscription};

/// What a pen event is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The pen touched the canvas.
    Down,
    /// It moved while touching.
    Move,
    /// It lifted.
    Up,
    /// It hovers over the canvas (not touching).
    Hover,
    /// It left the canvas or went out of range.
    Leave,
    /// The system took the pen away (e.g. a system gesture): drop what it drew.
    Cancel,
}

/// One pen sample, in window coordinates (logical pixels, like iced's layout).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub x: f32,
    pub y: f32,
    /// 0..1 (a mouse: 0.5).
    pub pressure: f32,
    /// Tilt towards x and y, degrees (-90..90).
    pub tilt: (f32, f32),
    /// Milliseconds on some clock (only differences matter).
    pub t_ms: u32,
}

/// A pen event with the samples it brings (oldest first; Down/Up/Hover: one).
#[derive(Debug, Clone, PartialEq)]
pub struct PenEvent {
    pub phase: Phase,
    pub samples: Vec<Sample>,
    /// The eraser end (or the eraser button) is in use.
    pub eraser: bool,
    /// The barrel (side) button is held.
    pub barrel: bool,
    /// From a real pen (false: a mouse standing in for one).
    pub real: bool,
}

impl PenEvent {
    /// The newest sample.
    pub fn last(&self) -> Option<Sample> {
        self.samples.last().copied()
    }
}

struct Shared {
    /// Where the pen draws (window logical px): pens going down elsewhere act like fingers.
    areas: Vec<Rectangle>,
    tx: Option<mpsc::UnboundedSender<PenEvent>>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { areas: Vec::new(), tx: None });
/// A pen is near the screen (hovering or touching): touches on the canvas are a resting palm.
static IN_RANGE: AtomicBool = AtomicBool::new(false);
static ATTACHED: AtomicBool = AtomicBool::new(false);

/// Sets the canvas areas (window logical px). An empty list sends every pen to winit (menus,
/// dialogs over the canvas).
pub fn set_ink_areas(areas: Vec<Rectangle>) {
    if let Ok(mut s) = SHARED.lock()
        && s.areas != areas
    {
        s.areas = areas;
    }
}

/// Whether a pen is near the screen right now (palm rejection: ignore touches meanwhile).
pub fn in_range() -> bool {
    IN_RANGE.load(Ordering::Relaxed)
}

/// Whether the pen backend is running (Windows, after [`attach`]).
pub fn attached() -> bool {
    ATTACHED.load(Ordering::Relaxed)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn send(e: PenEvent) {
    if let Ok(mut s) = SHARED.lock()
        && let Some(tx) = &s.tx
        && tx.unbounded_send(e).is_err()
    {
        s.tx = None;
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn in_areas(x: f32, y: f32) -> bool {
    SHARED.lock().map(|s| s.areas.iter().any(|r| r.contains(iced::Point::new(x, y)))).unwrap_or(false)
}

/// The pen events of the window [`attach`] was called for.
pub fn subscription() -> Subscription<PenEvent> {
    Subscription::run(|| {
        let (tx, rx) = mpsc::unbounded();
        if let Ok(mut s) = SHARED.lock() {
            s.tx = Some(tx);
        }
        rx
    })
}

/// Starts reading the pen of window `w` (Windows; elsewhere false and nothing happens). Runs on
/// the window's thread (`iced::window::run`).
pub fn attach(w: &dyn iced::window::Window) -> bool {
    #[cfg(windows)]
    {
        use raw_window_handle::RawWindowHandle;
        let Ok(handle) = w.window_handle() else { return false };
        let RawWindowHandle::Win32(h) = handle.as_raw() else { return false };
        if ATTACHED.swap(true, Ordering::SeqCst) {
            return true;
        }
        // SAFETY: the handle is the live top-level window of this thread.
        let ok = unsafe { win::install(h.hwnd.get() as *mut core::ffi::c_void) };
        if !ok {
            ATTACHED.store(false, Ordering::SeqCst);
        }
        ok
    }
    #[cfg(not(windows))]
    {
        let _ = w;
        false
    }
}

/// A pen sample's screen position (physical px) from its digitizer position: `h` (HIMETRIC)
/// mapped from the device's rectangle `d` onto the display's `s` (left, top, right, bottom) —
/// finer than a pixel. None when that lands more than a couple of pixels away from the whole
/// pixel the system reports for it (`px`): the mapping is stale (the screen was turned, a
/// monitor added, the resolution changed) or doesn't fit this pen.
#[cfg_attr(not(windows), allow(dead_code))]
fn map_pen(h: (i32, i32), d: [i32; 4], s: [i32; 4], px: (i32, i32)) -> Option<(f32, f32)> {
    let (dw, dh) = ((d[2] - d[0]) as f32, (d[3] - d[1]) as f32);
    if dw <= 0.0 || dh <= 0.0 || s[2] <= s[0] || s[3] <= s[1] {
        return None;
    }
    let x = s[0] as f32 + (h.0 - d[0]) as f32 * (s[2] - s[0]) as f32 / dw;
    let y = s[1] as f32 + (h.1 - d[1]) as f32 * (s[3] - s[1]) as f32 / dh;
    ((x - px.0 as f32).abs() <= 2.5 && (y - px.1 as f32).abs() <= 2.5).then_some((x, y))
}

#[cfg(test)]
mod tests {
    use super::map_pen;

    #[test]
    fn the_digitizer_maps_finer_than_a_pixel_unless_the_mapping_is_stale() {
        // A 1920 × 1080 screen whose digitizer reports 0..29_000 × 0..16_310 HIMETRIC.
        let (d, s) = ([0, 0, 29_000, 16_310], [0, 0, 1920, 1080]);
        let (x, y) = map_pen((14_500, 8_155), d, s, (960, 540)).expect("fits");
        assert!((x - 960.0).abs() < 0.01 && (y - 540.0).abs() < 0.01);
        let (x, _) = map_pen((14_510, 8_155), d, s, (960, 540)).expect("fits");
        assert!(x > 960.0 && x < 961.0, "between pixels: {x}");
        // The screen turned to portrait: the old mapping puts the pen elsewhere.
        assert_eq!(map_pen((14_500, 8_155), d, s, (540, 960)), None);
        assert_eq!(map_pen((1, 1), [0, 0, 0, 10], s, (0, 0)), None, "empty device rect");
    }
}

#[cfg(windows)]
mod win {
    //! The Win32 side: a window subclass that reads pen pointers.
    use super::{IN_RANGE, Ordering, PenEvent, Phase, Sample, in_areas, map_pen, send};
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::Controls::{
        FEEDBACK_PEN_BARRELVISUALIZATION, FEEDBACK_PEN_DOUBLETAP, FEEDBACK_PEN_PRESSANDHOLD, FEEDBACK_PEN_RIGHTTAP, FEEDBACK_PEN_TAP, FEEDBACK_TYPE, SetWindowFeedbackSetting,
    };
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::Input::Pointer::{
        GetPointerDeviceRects, GetPointerPenInfo, GetPointerPenInfoHistory, GetPointerType, POINTER_FLAG_CANCELED, POINTER_FLAG_INCONTACT, POINTER_FLAG_INRANGE, POINTER_PEN_INFO,
    };
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::TabletPC::{
        TABLET_DISABLE_FLICKFALLBACKKEYS, TABLET_DISABLE_FLICKS, TABLET_DISABLE_PENBARRELFEEDBACK, TABLET_DISABLE_PENTAPFEEDBACK, TABLET_DISABLE_PRESSANDHOLD,
        WM_TABLET_QUERYSYSTEMGESTURESTATUS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PEN_FLAG_BARREL, PEN_FLAG_ERASER, PEN_FLAG_INVERTED, POINTER_INPUT_TYPE, PT_PEN, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_POINTERCAPTURECHANGED, WM_POINTERDOWN, WM_POINTERLEAVE, WM_POINTERUP,
    };

    /// WM_POINTERUPDATE (not exported under this name by every windows-sys version).
    const WM_POINTERUPDATE: u32 = 0x0245;
    const SUBCLASS_ID: usize = 0x6e30_7465; // "n0te"

    struct Pen {
        /// The pointer drawing on the canvas right now (its messages are ours until it lifts).
        active: Option<u32>,
        /// Digitizer → screen mapping per device: (device rect HIMETRIC, display rect px).
        rects: HashMap<usize, Option<(RECT, RECT)>>,
    }

    static PEN: LazyLock<Mutex<Pen>> = LazyLock::new(|| Mutex::new(Pen { active: None, rects: HashMap::new() }));

    pub unsafe fn install(hwnd: HWND) -> bool {
        // SAFETY: plain Win32 calls on our own window.
        unsafe {
            let off: i32 = 0;
            for f in [FEEDBACK_PEN_BARRELVISUALIZATION, FEEDBACK_PEN_TAP, FEEDBACK_PEN_DOUBLETAP, FEEDBACK_PEN_PRESSANDHOLD, FEEDBACK_PEN_RIGHTTAP] {
                SetWindowFeedbackSetting(hwnd, f as FEEDBACK_TYPE, 0, std::mem::size_of::<i32>() as u32, &off as *const i32 as *const _);
            }
            SetWindowSubclass(hwnd, Some(proc_), SUBCLASS_ID, 0) != 0
        }
    }

    fn pointer_id(w: WPARAM) -> u32 {
        (w & 0xFFFF) as u32
    }

    /// Screen pixels (fractional) of a pen sample: HIMETRIC through the device's mapping when
    /// it fits the pixel location the system reports, else that integer pixel location. A
    /// mapping that stopped fitting (the screen turned, monitors changed) is asked for again
    /// once; one that still doesn't fit isn't used for that pen any more (until the displays
    /// change: [`proc_`] forgets the mappings then).
    fn screen_xy(info: &POINTER_PEN_INFO, rects: &mut HashMap<usize, Option<(RECT, RECT)>>) -> (f32, f32) {
        let p = &info.pointerInfo;
        let dev = p.sourceDevice as usize;
        let px = (p.ptPixelLocationRaw.x, p.ptPixelLocationRaw.y);
        let h = (p.ptHimetricLocationRaw.x, p.ptHimetricLocationRaw.y);
        let query = || {
            let mut d: RECT = unsafe { std::mem::zeroed() };
            let mut s: RECT = unsafe { std::mem::zeroed() };
            // SAFETY: out-pointers to locals.
            let ok = unsafe { GetPointerDeviceRects(p.sourceDevice as HANDLE, &mut d, &mut s) } != 0;
            (ok && d.right > d.left && d.bottom > d.top && s.right > s.left && s.bottom > s.top).then_some((d, s))
        };
        let r = |r: RECT| [r.left, r.top, r.right, r.bottom];
        let fits = |m: Option<(RECT, RECT)>| m.and_then(|(d, s)| map_pen(h, r(d), r(s), px));
        let cached = *rects.entry(dev).or_insert_with(query);
        if cached.is_some() {
            if let Some(xy) = fits(cached) {
                return xy;
            }
            let fresh = query();
            let xy = fits(fresh);
            rects.insert(dev, if xy.is_some() { fresh } else { None });
            if let Some(xy) = xy {
                return xy;
            }
        }
        (px.0 as f32, px.1 as f32)
    }

    /// A pen sample in window logical px.
    fn sample(hwnd: HWND, info: &POINTER_PEN_INFO, rects: &mut HashMap<usize, Option<(RECT, RECT)>>) -> Sample {
        let (sx, sy) = screen_xy(info, rects);
        let mut origin = POINT { x: 0, y: 0 };
        // SAFETY: our window, out-pointer to a local.
        let dpi = unsafe {
            ClientToScreen(hwnd, &mut origin);
            GetDpiForWindow(hwnd)
        };
        let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
        Sample {
            x: (sx - origin.x as f32) / scale,
            y: (sy - origin.y as f32) / scale,
            pressure: if info.pressure == 0 && info.pointerInfo.pointerFlags & POINTER_FLAG_INCONTACT != 0 { 0.5 } else { (info.pressure as f32 / 1024.0).clamp(0.0, 1.0) },
            tilt: (info.tiltX as f32, info.tiltY as f32),
            t_ms: info.pointerInfo.dwTime,
        }
    }

    fn flags(info: &POINTER_PEN_INFO) -> (bool, bool) {
        (info.penFlags & (PEN_FLAG_ERASER | PEN_FLAG_INVERTED) != 0, info.penFlags & PEN_FLAG_BARREL != 0)
    }

    unsafe extern "system" fn proc_(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM, _id: usize, _data: usize) -> LRESULT {
        // SAFETY: forwarding to the next window procedure with the same arguments.
        let pass = || unsafe { DefSubclassProc(hwnd, msg, w, l) };
        match msg {
            // Displays changed (turned, resolution, monitors, scaling): the pens' mappings onto
            // the screen are asked for again.
            WM_DISPLAYCHANGE | WM_DPICHANGED => {
                if let Ok(mut pen) = PEN.try_lock() {
                    pen.rects.clear();
                }
                pass()
            }
            WM_TABLET_QUERYSYSTEMGESTURESTATUS => {
                (TABLET_DISABLE_PRESSANDHOLD | TABLET_DISABLE_PENTAPFEEDBACK | TABLET_DISABLE_PENBARRELFEEDBACK | TABLET_DISABLE_FLICKS | TABLET_DISABLE_FLICKFALLBACKKEYS) as LRESULT
            }
            WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP | WM_POINTERLEAVE | WM_POINTERCAPTURECHANGED => {
                let id = pointer_id(w);
                let mut kind: POINTER_INPUT_TYPE = 0;
                // SAFETY: out-pointer to a local.
                if unsafe { GetPointerType(id, &mut kind) } == 0 || kind != PT_PEN {
                    return pass();
                }
                let Ok(mut pen) = PEN.try_lock() else { return pass() };
                let mut info: POINTER_PEN_INFO = unsafe { std::mem::zeroed() };
                // SAFETY: out-pointer to a local.
                if unsafe { GetPointerPenInfo(id, &mut info) } == 0 {
                    drop(pen);
                    return pass();
                }
                let pflags = info.pointerInfo.pointerFlags;
                IN_RANGE.store(pflags & POINTER_FLAG_INRANGE != 0 && msg != WM_POINTERLEAVE, Ordering::Relaxed);
                let (eraser, barrel) = flags(&info);
                let ours = pen.active == Some(id);
                match msg {
                    WM_POINTERDOWN => {
                        let s = sample(hwnd, &info, &mut pen.rects);
                        if !in_areas(s.x, s.y) {
                            drop(pen);
                            return pass();
                        }
                        pen.active = Some(id);
                        drop(pen);
                        send(PenEvent { phase: Phase::Down, samples: vec![s], eraser, barrel, real: true });
                        0
                    }
                    WM_POINTERUPDATE if ours => {
                        if pflags & POINTER_FLAG_CANCELED != 0 {
                            pen.active = None;
                            drop(pen);
                            send(PenEvent { phase: Phase::Cancel, samples: Vec::new(), eraser, barrel, real: true });
                            return 0;
                        }
                        // Every sample since the last message (the history is newest first).
                        let mut n = info.pointerInfo.historyCount.max(1);
                        let mut hist = vec![unsafe { std::mem::zeroed::<POINTER_PEN_INFO>() }; n as usize];
                        // SAFETY: the buffer holds n entries; n is updated to what was written.
                        let got = unsafe { GetPointerPenInfoHistory(id, &mut n, hist.as_mut_ptr()) } != 0;
                        let rects = &mut pen.rects;
                        let samples: Vec<Sample> = if got { hist[..n as usize].iter().rev().map(|i| sample(hwnd, i, rects)).collect() } else { vec![sample(hwnd, &info, rects)] };
                        drop(pen);
                        send(PenEvent { phase: Phase::Move, samples, eraser, barrel, real: true });
                        0
                    }
                    WM_POINTERUPDATE => {
                        // Hovering (or touching outside the canvas: winit's).
                        if pflags & POINTER_FLAG_INCONTACT == 0 {
                            let s = sample(hwnd, &info, &mut pen.rects);
                            drop(pen);
                            let phase = if in_areas(s.x, s.y) { Phase::Hover } else { Phase::Leave };
                            send(PenEvent { phase, samples: vec![s], eraser, barrel, real: true });
                        } else {
                            drop(pen);
                        }
                        pass()
                    }
                    WM_POINTERUP if ours => {
                        let s = sample(hwnd, &info, &mut pen.rects);
                        pen.active = None;
                        drop(pen);
                        send(PenEvent { phase: Phase::Up, samples: vec![s], eraser, barrel, real: true });
                        0
                    }
                    WM_POINTERCAPTURECHANGED if ours => {
                        pen.active = None;
                        drop(pen);
                        send(PenEvent { phase: Phase::Cancel, samples: Vec::new(), eraser, barrel, real: true });
                        0
                    }
                    WM_POINTERLEAVE => {
                        drop(pen);
                        send(PenEvent { phase: Phase::Leave, samples: Vec::new(), eraser, barrel, real: true });
                        pass()
                    }
                    _ => {
                        drop(pen);
                        pass()
                    }
                }
            }
            _ => pass(),
        }
    }
}
