//! n0te laptop app (iced 0.14 spike): Nothing-style chrome, a page canvas with lots of ink,
//! a block-based flowing text page and the notebook outline.

pub mod app;
pub mod blocks;
pub mod demo;
pub mod pages;
pub mod sidebar;
pub mod theme;

pub use app::{App, Message};

/// Runs the app in a window.
pub fn run() -> iced::Result {
    let mut app = iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .window_size((1440.0, 900.0))
        .antialiasing(true)
        .default_font(theme::SANS);
    for font in theme::FONT_DATA {
        app = app.font(font);
    }
    app.run()
}
