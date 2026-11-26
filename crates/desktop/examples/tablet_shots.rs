//! The README's n0te Tablet pictures, taken headless: the app runs as n0te Tablet, the mouse
//! stands in for the pen (as on Linux), and the tablet's tools draw on the demo notebook:
//!
//!   cargo run --profile fast -p n0te-desktop --example tablet_shots -- docs/screenshots
//!
//! Writes tablet.png (shapes that snapped, an arrow, a lasso selection, the tool bar) and
//! tablet-pens.png (the pen menu). Uses its own temporary library, config and runtime folders.

#[path = "../tests/harness/mod.rs"]
mod harness;

use std::path::PathBuf;

use harness::Harness;
use iced::{Point, Rectangle, Size};
use n0te_core::store::Library;
use n0te_desktop::app::{Message, Screen};
use n0te_desktop::tablet::{Msg, Tool};
use n0te_desktop::{App, Mode, demo};

fn main() {
    let out = std::path::absolute(PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "docs/screenshots".into()))).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let scratch = std::env::temp_dir().join(format!("n0te-tablet-shots-{}", std::process::id()));
    for d in ["run", "config", "cache"] {
        std::fs::create_dir_all(scratch.join(d)).unwrap();
    }
    // SAFETY: still one thread; nothing has read the environment yet.
    unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", scratch.join("run"));
        std::env::set_var("XDG_CONFIG_HOME", scratch.join("config"));
        std::env::set_var("XDG_CACHE_HOME", scratch.join("cache"));
    }
    n0te_desktop::set_mode(Mode::Tablet);
    let root = scratch.join("library");
    let id = demo::save_into(&Library::open(&root).unwrap());
    let backend = std::env::var("N0TE_BACKEND").unwrap_or_else(|_| "wgpu".into());
    let mut h = Harness::new(App::new(&root), &backend, Size::new(1400.0, 860.0), 1.0).expect("renderer");
    h.message(Message::Open(id));
    for _ in 0..3 {
        h.events(&[]);
    }
    // The cover page's empty lower half.
    let at = |h: &Harness, fx: f32, fy: f32| -> Point {
        let Screen::Notebook(o) = &h.app.screen else { panic!("no notebook") };
        let p = o.pages.pages.iter().find(|p| p.placed.side == 0).expect("a page");
        let r = p.placed.rect;
        let cam = o.camera;
        cam.to_screen(Rectangle::new(cam.origin, cam.viewport), r.x0 + r.width() * fx, r.y0 + r.height() * fy)
    };
    let hold = |h: &mut Harness| {
        std::thread::sleep(std::time::Duration::from_millis(650));
        h.message(Message::Tablet(Msg::Frame));
        std::thread::sleep(std::time::Duration::from_millis(400));
        h.message(Message::Tablet(Msg::Frame));
    };
    let path = |h: &mut Harness, pts: &[Point], rest: bool| {
        h.press_at(pts[0]);
        for p in &pts[1..] {
            h.move_to(*p);
        }
        if rest {
            hold(h);
        }
        h.release();
    };
    let wobble = |from: Point, to: Point, n: usize, amp: f32| -> Vec<Point> {
        (0..=n)
            .map(|i| {
                let t = i as f32 / n as f32;
                let w = if i % 2 == 0 { amp } else { -amp };
                Point::new(from.x + (to.x - from.x) * t + w * 0.3, from.y + (to.y - from.y) * t + w)
            })
            .collect()
    };
    // A rough rectangle, held: it snaps clean.
    let (a, b) = (at(&h, 0.12, 0.62), at(&h, 0.42, 0.78));
    let mut rect = wobble(a, Point::new(b.x, a.y), 10, 2.0);
    rect.extend(wobble(Point::new(b.x, a.y), b, 6, 2.0));
    rect.extend(wobble(b, Point::new(a.x, b.y), 10, 2.0));
    rect.extend(wobble(Point::new(a.x, b.y), a, 6, 2.0));
    path(&mut h, &rect, true);
    // A rough circle in blue, held.
    h.message(Message::Tablet(Msg::Color(1)));
    let c = at(&h, 0.62, 0.70);
    let circle: Vec<Point> = (0..=40).map(|i| {
        let t = i as f32 / 40.0 * std::f32::consts::TAU;
        let r = 46.0 + if i % 3 == 0 { 2.5 } else { -1.5 };
        Point::new(c.x + r * t.cos(), c.y + r * 0.8 * t.sin())
    }).collect();
    path(&mut h, &circle, true);
    // A wobbly line held straight, then a V at its end: an arrow.
    h.message(Message::Tablet(Msg::Color(2)));
    let (l0, l1) = (at(&h, 0.12, 0.53), at(&h, 0.46, 0.53));
    path(&mut h, &wobble(l0, l1, 24, 2.5), true);
    path(&mut h, &[Point::new(l1.x - 14.0, l1.y - 12.0), Point::new(l1.x - 6.0, l1.y - 5.0), l1, Point::new(l1.x - 7.0, l1.y + 6.0), Point::new(l1.x - 14.0, l1.y + 12.0)], true);
    // Some writing in black, then the lasso around it.
    h.message(Message::Tablet(Msg::Color(0)));
    let w0 = at(&h, 0.6, 0.53);
    for k in 0..5 {
        let x = w0.x + k as f32 * 22.0;
        let pts: Vec<Point> = (0..=12).map(|i| Point::new(x + i as f32 * 1.4, w0.y + if i % 4 < 2 { -9.0 } else { 6.0 })).collect();
        path(&mut h, &pts, false);
    }
    h.message(Message::Tablet(Msg::Tool(Tool::Lasso)));
    let loop_pts: Vec<Point> = (0..=36).map(|i| {
        let t = i as f32 / 36.0 * std::f32::consts::TAU;
        Point::new(w0.x + 55.0 + 80.0 * t.cos(), w0.y + 26.0 * t.sin())
    }).collect();
    path(&mut h, &loop_pts, false);
    for _ in 0..2 {
        h.events(&[]);
    }
    h.screenshot(out.join("tablet.png"));
    println!("tablet.png");
    // A small pen laptop (1200 × 800 logical px, e.g. 1800 × 1200 at 150 %): the bar must fit.
    if std::env::var_os("N0TE_SHOTS_NARROW").is_some() {
        let wide = h.size;
        h.size = Size::new(1200.0, 800.0);
        for _ in 0..3 {
            h.events(&[]);
        }
        h.screenshot(out.join("tablet-narrow.png"));
        println!("tablet-narrow.png");
        h.message(Message::Tablet(Msg::Tool(Tool::Eraser)));
        h.events(&[]);
        h.screenshot(out.join("tablet-narrow-eraser.png"));
        println!("tablet-narrow-eraser.png");
        h.message(Message::Tablet(Msg::Tool(Tool::Lasso)));
        h.size = wide;
        for _ in 0..3 {
            h.events(&[]);
        }
    }
    h.message(Message::Tablet(Msg::ClearSelection));
    h.message(Message::Tablet(Msg::Tool(Tool::Pen)));
    h.message(Message::Tablet(Msg::PenMenu(true)));
    h.events(&[]);
    h.screenshot(out.join("tablet-pens.png"));
    println!("tablet-pens.png");
    drop(h);
    let _ = std::fs::remove_dir_all(&scratch);
}
