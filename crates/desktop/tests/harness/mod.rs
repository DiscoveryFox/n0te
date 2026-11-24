//! A small synchronous headless runtime for the app: builds the UI from `App::view`, feeds it
//! events, runs messages and tasks (incl. widget operations such as focus), draws, and takes
//! screenshots with iced's headless renderers (tiny-skia on the CPU or wgpu without a window).
//! No window is ever opened.
#![allow(dead_code)]

use std::borrow::Cow;
use std::path::Path;
use std::time::Instant;

use iced_test::core::renderer::Headless as _;
use iced_test::core::{self, Event, Point, Size, SmolStr, clipboard, keyboard, mouse, window};
use iced_test::futures::futures::{StreamExt, executor};
use iced_test::renderer::Renderer;
use iced_test::runtime::{self, UserInterface, user_interface};

use n0te_desktop::{App, Message, theme};

#[derive(Default)]
pub struct MemClipboard {
    pub standard: Option<String>,
    pub primary: Option<String>,
}

impl core::Clipboard for MemClipboard {
    fn read(&self, kind: clipboard::Kind) -> Option<String> {
        match kind {
            clipboard::Kind::Standard => self.standard.clone(),
            clipboard::Kind::Primary => self.primary.clone(),
        }
    }
    fn write(&mut self, kind: clipboard::Kind, contents: String) {
        match kind {
            clipboard::Kind::Standard => self.standard = Some(contents),
            clipboard::Kind::Primary => self.primary = Some(contents),
        }
    }
}

/// Times of one frame in milliseconds.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameTimes {
    /// Event handling + `App::update` for the produced messages.
    pub update: f32,
    /// `App::view` + layout + widget tree diff.
    pub view: f32,
    /// Widget `draw` (records primitives; includes tessellating uncached pages).
    pub draw: f32,
    /// Rendering the primitives offscreen (tiny-skia raster / wgpu submit + read back).
    pub render: f32,
}

impl FrameTimes {
    pub fn cpu(&self) -> f32 {
        self.update + self.view + self.draw
    }
    pub fn total(&self) -> f32 {
        self.update + self.view + self.draw + self.render
    }
}

pub struct Harness {
    pub app: App,
    pub renderer: Renderer,
    cache: Option<user_interface::Cache>,
    pub size: Size,
    pub scale: f32,
    pub cursor: mouse::Cursor,
    pub clipboard: MemClipboard,
    pub theme: iced::Theme,
    pub modifiers: keyboard::Modifiers,
}

/// Tests and the screenshot example open throwaway libraries under the temp dir
/// (`n0te-…`); they go when the harness does, so test runs don't fill /tmp.
impl Drop for Harness {
    fn drop(&mut self) {
        let root = self.app.root.clone();
        let tmp = std::env::temp_dir();
        if root.starts_with(&tmp) && root.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("n0te-")) {
            let _ = std::fs::remove_dir_all(&root);
        }
    }
}

pub fn load_fonts() {
    let mut fs = iced_test::renderer::graphics::text::font_system().write().expect("font system");
    for f in theme::FONT_DATA {
        fs.load_font(Cow::Borrowed(f));
    }
}

impl Harness {
    /// `backend`: "tiny-skia" or "wgpu". Returns None if that backend is unavailable.
    pub fn new(app: App, backend: &str, size: Size, scale: f32) -> Option<Self> {
        load_fonts();
        let renderer = executor::block_on(<Renderer as core::renderer::Headless>::new(theme::SANS, core::Pixels(16.0), Some(backend)))?;
        let mut h = Harness {
            app,
            renderer,
            cache: Some(user_interface::Cache::default()),
            size,
            scale,
            cursor: mouse::Cursor::Unavailable,
            clipboard: MemClipboard::default(),
            theme: theme::theme(),
            modifiers: keyboard::Modifiers::default(),
        };
        // First layout pass: lets the page view report its size to the app.
        h.events(&[]);
        h.events(&[]);
        Some(h)
    }

    /// Bounds of the widget with `id` in the current UI.
    pub fn bounds_of(&mut self, id: iced::widget::Id) -> Option<core::Rectangle> {
        use core::widget::Operation;
        use iced_test::selector::Selector;
        let mut op = id.find();
        let mut ui = UserInterface::build(self.app.view(), self.size, self.cache.take().unwrap_or_default(), &mut self.renderer);
        ui.operate(&self.renderer, &mut core::widget::operation::black_box(&mut op));
        self.cache = Some(ui.into_cache());
        match op.finish() {
            core::widget::operation::Outcome::Some(Some(t)) => Some(t.bounds()),
            _ => None,
        }
    }

    /// Bounds of the first widget `selector` picks in the current UI (a text, an id, the
    /// focused field: see `iced_test::selector`).
    pub fn find<S>(&mut self, selector: S) -> Option<core::Rectangle>
    where
        S: iced_test::selector::Selector + Send,
        S::Output: iced_test::selector::Bounded + Clone + Send,
    {
        use core::widget::Operation;
        use iced_test::selector::Bounded as _;
        let mut op = selector.find();
        let mut ui = UserInterface::build(self.app.view(), self.size, self.cache.take().unwrap_or_default(), &mut self.renderer);
        ui.operate(&self.renderer, &mut core::widget::operation::black_box(&mut op));
        self.cache = Some(ui.into_cache());
        match op.finish() {
            core::widget::operation::Outcome::Some(Some(t)) => Some(t.bounds()),
            _ => None,
        }
    }

    /// Runs a task returned outside of `update` (e.g. a focus operation).
    pub fn task(&mut self, task: iced::Task<Message>) {
        self.run_task(task);
    }

    pub fn backend(&self) -> String {
        self.renderer.name()
    }

    /// Feeds events through the UI and runs the resulting messages (and their tasks).
    pub fn events(&mut self, events: &[Event]) -> f32 {
        let started = Instant::now();
        let mut messages = Vec::new();
        let mut ui = UserInterface::build(self.app.view(), self.size, self.cache.take().unwrap_or_default(), &mut self.renderer);
        let mut all = events.to_vec();
        all.push(Event::Window(window::Event::RedrawRequested(Instant::now())));
        let _ = ui.update(&all, self.cursor, &mut self.renderer, &mut self.clipboard, &mut messages);
        self.cache = Some(ui.into_cache());
        for m in messages {
            self.message(m);
        }
        started.elapsed().as_secs_f32() * 1000.0
    }

    /// Runs one message through `App::update`, then its task (outputs, widget operations).
    pub fn message(&mut self, m: Message) {
        let task = self.app.update(m);
        self.run_task(task);
    }

    fn run_task(&mut self, task: iced::Task<Message>) {
        let Some(mut stream) = runtime::task::into_stream(task) else { return };
        // One action at a time, like the runtime: a task that waits for what its widget
        // operation finds only ends once that operation has run.
        while let Some(action) = executor::block_on(stream.next()) {
            match action {
                runtime::Action::Output(m) => self.message(m),
                runtime::Action::Widget(op) => {
                    let mut ui = UserInterface::build(self.app.view(), self.size, self.cache.take().unwrap_or_default(), &mut self.renderer);
                    let mut op = Some(op);
                    while let Some(mut current) = op.take() {
                        ui.operate(&self.renderer, current.as_mut());
                        if let core::widget::operation::Outcome::Chain(next) = current.finish() {
                            op = Some(next);
                        }
                    }
                    self.cache = Some(ui.into_cache());
                }
                _ => {}
            }
        }
    }

    /// Lays out, draws and renders one frame offscreen; returns the RGBA pixels and timings.
    pub fn frame_rgba(&mut self) -> (Vec<u8>, FrameTimes) {
        let mut t = FrameTimes::default();
        let s = Instant::now();
        let mut ui = UserInterface::build(self.app.view(), self.size, self.cache.take().unwrap_or_default(), &mut self.renderer);
        t.view = s.elapsed().as_secs_f32() * 1000.0;
        let s = Instant::now();
        ui.draw(&mut self.renderer, &self.theme, &core::renderer::Style { text_color: theme::p().text }, self.cursor);
        t.draw = s.elapsed().as_secs_f32() * 1000.0;
        self.cache = Some(ui.into_cache());
        let s = Instant::now();
        let physical = Size::new((self.size.width * self.scale).round() as u32, (self.size.height * self.scale).round() as u32);
        let rgba = self.renderer.screenshot(physical, self.scale, theme::p().chrome);
        t.render = s.elapsed().as_secs_f32() * 1000.0;
        (rgba, t)
    }

    pub fn frame(&mut self) -> FrameTimes {
        self.frame_rgba().1
    }

    /// One interactive frame: events (+ update) then draw/render, like the real event loop.
    pub fn step(&mut self, events: &[Event]) -> FrameTimes {
        let update = self.events(events);
        let mut t = self.frame();
        t.update = update;
        t
    }

    pub fn screenshot(&mut self, path: impl AsRef<Path>) {
        // Twice: the status bar shows the previous frame's draw statistics.
        self.frame();
        let (rgba, _) = self.frame_rgba();
        let physical = Size::new((self.size.width * self.scale).round() as u32, (self.size.height * self.scale).round() as u32);
        save_png(path.as_ref(), &rgba, physical.width, physical.height);
    }

    // --- input helpers ----------------------------------------------------------------------

    pub fn move_to(&mut self, p: Point) {
        self.cursor = mouse::Cursor::Available(p);
        self.events(&[Event::Mouse(mouse::Event::CursorMoved { position: p })]);
    }

    pub fn click(&mut self, p: Point) {
        self.move_to(p);
        self.events(&[Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))]);
    }

    pub fn right_click(&mut self, p: Point) {
        self.move_to(p);
        self.events(&[Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)), Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right))]);
    }

    /// Holds the left button down at `p` (a drag follows with `move_to`, then `release`).
    pub fn press_at(&mut self, p: Point) {
        self.move_to(p);
        self.events(&[Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))]);
    }

    pub fn release(&mut self) {
        self.events(&[Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))]);
    }

    pub fn set_modifiers(&mut self, m: keyboard::Modifiers) {
        self.modifiers = m;
        self.events(&[Event::Keyboard(keyboard::Event::ModifiersChanged(m))]);
    }

    pub fn key_event(&self, key: keyboard::Key, text: Option<&str>) -> [Event; 2] {
        let text = text.map(SmolStr::new);
        [
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key.clone(),
                physical_key: keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
                location: keyboard::Location::Standard,
                modifiers: self.modifiers,
                repeat: false,
                text,
            }),
            Event::Keyboard(keyboard::Event::KeyReleased {
                key: key.clone(),
                modified_key: key,
                physical_key: keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
                location: keyboard::Location::Standard,
                modifiers: self.modifiers,
            }),
        ]
    }

    pub fn press(&mut self, named: keyboard::key::Named) {
        let ev = self.key_event(keyboard::Key::Named(named), None);
        self.events(&ev);
    }

    /// Ctrl + character (e.g. 'z', 'c'); no text, like winit with Ctrl held.
    pub fn ctrl(&mut self, c: char) {
        let old = self.modifiers;
        self.set_modifiers(old | keyboard::Modifiers::CTRL);
        let ev = self.key_event(keyboard::Key::Character(SmolStr::new(c.to_string())), None);
        self.events(&ev);
        self.set_modifiers(old);
    }

    /// Types text the way a German keyboard layout delivers it: one key press per character
    /// with the character as the key's text (ä, ö, ü, ß come as plain text, no IME).
    pub fn type_text(&mut self, s: &str) {
        for c in s.chars() {
            let t = c.to_string();
            let ev = self.key_event(keyboard::Key::Character(SmolStr::new(&t)), Some(&t));
            self.events(&ev);
        }
    }

    /// IME composition: pre-edit updates, then the commit (what fcitx/ibus would send).
    pub fn ime(&mut self, preedit: &[&str], commit: &str) {
        use core::input_method::Event as Ime;
        self.events(&[Event::InputMethod(Ime::Opened)]);
        for p in preedit {
            self.events(&[Event::InputMethod(Ime::Preedit(p.to_string(), Some(0..p.len())))]);
        }
        self.events(&[Event::InputMethod(Ime::Commit(commit.to_string())), Event::InputMethod(Ime::Closed)]);
    }

    pub fn scroll(&mut self, at: Point, dx: f32, dy: f32) -> FrameTimes {
        self.cursor = mouse::Cursor::Available(at);
        self.step(&[Event::Mouse(mouse::Event::WheelScrolled { delta: mouse::ScrollDelta::Pixels { x: dx, y: dy } })])
    }
}

pub fn save_png(path: &Path, rgba: &[u8], w: u32, h: u32) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("shots dir");
    }
    let file = std::fs::File::create(path).expect("png file");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("png header");
    writer.write_image_data(rgba).expect("png data");
}

/// Resident set size and its peak, in MiB (Linux).
pub fn rss_mib() -> (f32, f32) {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| {
        s.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<f32>().ok())
            .map(|kb| kb / 1024.0)
            .unwrap_or(0.0)
    };
    (field("VmRSS:"), field("VmHWM:"))
}

pub fn stats(v: &mut [f32]) -> (f32, f32, f32) {
    if v.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p = |q: f32| v[((v.len() - 1) as f32 * q).round() as usize];
    (p(0.5), p(0.95), v[v.len() - 1])
}
