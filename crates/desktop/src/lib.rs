//! n0te laptop app (iced 0.14): the library and the notebook view on the local n0te library,
//! laid out and drawn like on the tablet; `.n0te` bundles carry notebooks between devices.

pub mod app;
pub mod blocks;
pub mod demo;
pub mod library;
pub mod markdown;
pub mod pages;
pub mod pdf;
pub mod sidebar;
pub mod theme;
pub mod widgets;

pub use app::{App, Message};

/// Runs the app in a window. `import`: a `.n0te` file to import first (from the command line).
pub fn run(import: Option<std::path::PathBuf>) -> iced::Result {
    let root = app::default_root();
    let mut app = iced::application(move || App::boot_with(root.clone(), import.clone()), App::update, App::view)
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
