//! Touchpad gestures on Wayland: pinch to zoom, and the end of a two-finger swipe (for the page
//! to glide on).
//!
//! iced (winit 0.30) turns two-finger scrolling into wheel events, but passes no pinch on Linux
//! and never says when the fingers leave the touchpad. So we share the app's Wayland connection
//! (its `wl_display`, with an event queue of our own on a thread) and listen ourselves:
//! `zwp_pointer_gestures_v1` for pinch and hold, and a second `wl_pointer` of the seat for the
//! pointer position and `axis_stop` (fingers lifted after scrolling). Elsewhere (X11) it stays
//! off and everything else works as before.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::futures::Stream;
use iced::futures::channel::mpsc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    /// Two fingers start pinching with the pointer at (x, y) (window, logical pixels).
    PinchBegin { x: f32, y: f32 },
    /// `scale` since the begin; the fingers' centre moved by (dx, dy) since the last update.
    Pinch { scale: f32, dx: f32, dy: f32 },
    PinchEnd,
    /// Fingers lifted after a two-finger swipe moving (vx, vy) pixels per second, in scroll
    /// direction (positive: the view moves right / down).
    Fling { vx: f32, vy: f32 },
    /// Fingers are on the touchpad again (resting, or a new swipe): stop gliding.
    Touch,
}

static SENDER: Mutex<Option<mpsc::UnboundedSender<Gesture>>> = Mutex::new(None);
static STARTED: AtomicBool = AtomicBool::new(false);

/// The gestures as they come.
pub fn subscription() -> iced::Subscription<Gesture> {
    iced::Subscription::run(stream)
}

fn stream() -> impl Stream<Item = Gesture> {
    let (tx, rx) = mpsc::unbounded();
    *SENDER.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(tx);
    rx
}

/// `N0TE_GESTURE_LOG=1`: print what arrives (to check a touchpad without a debugger).
fn logging() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("N0TE_GESTURE_LOG").is_some())
}

fn send(g: Gesture) {
    if logging() {
        eprintln!("n0te gesture: {g:?}");
    }
    if let Some(tx) = SENDER.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
        let _ = tx.unbounded_send(g);
    }
}

/// The window's Wayland display (`wl_display*`), if it runs on Wayland.
pub fn wayland_display(w: &dyn iced::window::Window) -> Option<usize> {
    use raw_window_handle::RawDisplayHandle;
    let raw = w.display_handle().ok()?.as_raw();
    if logging() {
        eprintln!("n0te gesture: display {raw:?}");
    }
    match raw {
        RawDisplayHandle::Wayland(h) => Some(h.display.as_ptr() as usize),
        _ => None,
    }
}

/// Starts listening on the app's Wayland display; only the first call does anything.
pub fn start(display: usize) {
    if display == 0 || STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    #[cfg(target_os = "linux")]
    std::thread::Builder::new()
        .name("n0te-gestures".into())
        .spawn(move || {
            if let Err(e) = wl::run(display) {
                eprintln!("n0te: Touchpad-Gesten aus: {e}");
            }
        })
        .ok();
}

/// Velocity at the end of a swipe from its last samples (time ms, dx, dy): the motion of the
/// last 100 ms. None when the fingers had come to rest before lifting.
pub fn fling_velocity(samples: &VecDeque<(u32, f32, f32)>, stop: u32) -> Option<(f32, f32)> {
    let last = samples.back()?.0;
    if stop.wrapping_sub(last) > 60 {
        return None; // rested, then lifted
    }
    let from = stop.wrapping_sub(100);
    let window: Vec<_> = samples.iter().filter(|(t, _, _)| t.wrapping_sub(from) <= 100).collect();
    let first = window.first()?.0;
    let span = (stop.wrapping_sub(first)).max(16) as f32 / 1000.0;
    let (sx, sy) = window.iter().fold((0.0, 0.0), |(x, y), (_, dx, dy)| (x + dx, y + dy));
    let (vx, vy) = (sx / span, sy / span);
    (vx.hypot(vy) > 150.0).then_some((vx, vy))
}

#[cfg(target_os = "linux")]
mod wl {
    use super::{Gesture, VecDeque, fling_velocity, send};
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::{wl_pointer, wl_registry, wl_seat};
    use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
    use wayland_protocols::wp::pointer_gestures::zv1::client::{
        zwp_pointer_gesture_hold_v1 as hold, zwp_pointer_gesture_pinch_v1 as pinch, zwp_pointer_gestures_v1 as gestures,
    };

    #[derive(Default)]
    struct State {
        gestures: Option<gestures::ZwpPointerGesturesV1>,
        pointer: Option<wl_pointer::WlPointer>,
        pinch: Option<pinch::ZwpPointerGesturePinchV1>,
        hold: Option<hold::ZwpPointerGestureHoldV1>,
        /// Pointer position on our window (logical pixels).
        x: f32,
        y: f32,
        /// The current axis frame: from fingers, its motion and time, and whether it ends a swipe.
        finger: bool,
        dx: f32,
        dy: f32,
        time: u32,
        stop: Option<u32>,
        /// A two-finger swipe is under way; its last samples (time ms, dx, dy).
        swiping: bool,
        samples: VecDeque<(u32, f32, f32)>,
    }

    pub fn run(display: usize) -> Result<(), String> {
        // SAFETY: the pointer is winit's live `wl_display`, which outlives the app's windows; a
        // foreign display is only borrowed (never closed) and gets its own event queue.
        let backend = unsafe { wayland_backend::client::Backend::from_foreign_display(display as *mut _) };
        let conn = Connection::from_backend(backend);
        let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|e| e.to_string())?;
        let qh = queue.handle();
        let mut state = State::default();
        state.gestures = globals.bind::<gestures::ZwpPointerGesturesV1, _, _>(&qh, 1..=3, ()).ok();
        if state.gestures.is_none() {
            return Err("der Compositor bietet keine Zeigergesten an".into());
        }
        let _seat: wl_seat::WlSeat = globals.bind(&qh, 5..=8, ()).map_err(|e| format!("kein Eingabegerät: {e}"))?;
        if super::logging() {
            eprintln!("n0te gesture: listening (pointer gestures v{})", state.gestures.as_ref().map_or(0, |g| g.version()));
        }
        loop {
            queue.blocking_dispatch(&mut state).map_err(|e| e.to_string())?;
        }
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
        fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
    }

    impl Dispatch<wl_seat::WlSeat, ()> for State {
        fn event(state: &mut Self, seat: &wl_seat::WlSeat, event: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
            let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event else { return };
            if caps.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                let pointer = seat.get_pointer(qh, ());
                if let Some(g) = &state.gestures {
                    state.pinch = Some(g.get_pinch_gesture(&pointer, qh, ()));
                    if g.version() >= 3 {
                        state.hold = Some(g.get_hold_gesture(&pointer, qh, ()));
                    }
                }
                state.pointer = Some(pointer);
            }
        }
    }

    impl Dispatch<wl_pointer::WlPointer, ()> for State {
        fn event(s: &mut Self, _: &wl_pointer::WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            use wl_pointer::Event as E;
            match event {
                E::Enter { surface_x, surface_y, .. } | E::Motion { surface_x, surface_y, .. } => {
                    s.x = surface_x as f32;
                    s.y = surface_y as f32;
                }
                E::AxisSource { axis_source } => s.finger = matches!(axis_source, WEnum::Value(wl_pointer::AxisSource::Finger)),
                E::Axis { time, axis, value } => {
                    s.time = time;
                    match axis {
                        WEnum::Value(wl_pointer::Axis::VerticalScroll) => s.dy += value as f32,
                        WEnum::Value(wl_pointer::Axis::HorizontalScroll) => s.dx += value as f32,
                        _ => {}
                    }
                }
                E::AxisStop { time, .. } => s.stop = Some(time),
                E::Frame => frame(s),
                _ => {}
            }
        }
    }

    /// One pointer frame: a step of a two-finger swipe, or its end.
    fn frame(s: &mut State) {
        let moved = s.dx != 0.0 || s.dy != 0.0;
        if s.finger && moved {
            if !s.swiping {
                s.swiping = true;
                s.samples.clear();
                send(Gesture::Touch); // a new swipe catches a gliding page
            }
            s.samples.push_back((s.time, s.dx, s.dy));
            while s.samples.front().is_some_and(|(t, _, _)| s.time.wrapping_sub(*t) > 150) {
                s.samples.pop_front();
            }
        }
        if let Some(stop) = s.stop.take()
            && s.swiping
        {
            s.swiping = false;
            if let Some((vx, vy)) = fling_velocity(&s.samples, stop) {
                send(Gesture::Fling { vx, vy });
            }
            s.samples.clear();
        }
        s.dx = 0.0;
        s.dy = 0.0;
        s.finger = false;
    }

    impl Dispatch<gestures::ZwpPointerGesturesV1, ()> for State {
        fn event(_: &mut Self, _: &gestures::ZwpPointerGesturesV1, _: gestures::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    }

    impl Dispatch<pinch::ZwpPointerGesturePinchV1, ()> for State {
        fn event(s: &mut Self, _: &pinch::ZwpPointerGesturePinchV1, event: pinch::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            match event {
                pinch::Event::Begin { .. } => send(Gesture::PinchBegin { x: s.x, y: s.y }),
                pinch::Event::Update { dx, dy, scale, .. } => send(Gesture::Pinch { scale: scale as f32, dx: dx as f32, dy: dy as f32 }),
                pinch::Event::End { .. } => send(Gesture::PinchEnd),
                _ => {}
            }
        }
    }

    impl Dispatch<hold::ZwpPointerGestureHoldV1, ()> for State {
        fn event(_: &mut Self, _: &hold::ZwpPointerGestureHoldV1, event: hold::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            if let hold::Event::Begin { .. } = event {
                send(Gesture::Touch);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quick_swipe_flings_and_a_rested_one_does_not() {
        // 10 px every 10 ms downwards, fingers lifted right after: 1000 px/s.
        let samples: VecDeque<_> = (0..10).map(|i| (1000 + i * 10, 0.0, 10.0)).collect();
        let (vx, vy) = fling_velocity(&samples, 1095).unwrap();
        assert_eq!(vx, 0.0);
        assert!((900.0..=1300.0).contains(&vy), "{vy}");
        // The same swipe, but the fingers rested 200 ms before lifting: no glide.
        assert_eq!(fling_velocity(&samples, 1290), None);
        // Slow drifting is no fling either.
        let slow: VecDeque<_> = (0..10).map(|i| (1000 + i * 10, 0.5, 0.5)).collect();
        assert_eq!(fling_velocity(&slow, 1095), None);
        assert_eq!(fling_velocity(&VecDeque::new(), 5), None);
    }
}
