//! n0te laptop app (iced 0.14): the library and the notebook view on the local n0te library,
//! laid out and drawn like on the tablet; `.n0te` bundles carry notebooks between devices.

pub mod app;
pub mod blocks;
pub mod demo;
pub mod gestures;
pub mod i18n;
pub mod latex;
pub mod library;
pub mod live;
pub mod markdown;
pub mod mdstyle;
pub mod pages;
pub mod palette;
pub mod pdf;
pub mod plots;
pub mod saver;
pub mod search;
pub mod settings;
pub mod shelf;
pub mod sidebar;
pub mod split;
pub mod tablet;
pub mod theme;
pub mod updater;
pub mod webcam;
pub mod widgets;

pub use app::{App, Message};

/// Which of the two programs this is: the same app, set up for keyboard and mouse ("n0te
/// Laptop", the `n0te` binary) or for pen and touch ("n0te Tablet", `n0te-tablet`, the port of
/// the Android tablet app).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Laptop,
    Tablet,
}

static MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();

/// The program's mode (Laptop unless [`run_as`] said otherwise; tests run as Laptop).
pub fn mode() -> Mode {
    *MODE.get().unwrap_or(&Mode::Laptop)
}

static START_LINK: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// An invite link the program was started with (`n0te://invite/…`, e.g. a click on it in the
/// browser: the installer registers n0te:// for the apps). The join dialog opens with it; joining
/// still takes a click.
pub fn set_start_link(link: &str) {
    if link.starts_with("n0te://") {
        let _ = START_LINK.set(link.trim().to_string());
    }
}

pub fn start_link() -> Option<&'static str> {
    START_LINK.get().map(String::as_str)
}

/// Sets the program's mode before the app starts (false if it was set already). [`run_as`]
/// does this; tests of n0te Tablet call it first.
pub fn set_mode(mode: Mode) -> bool {
    MODE.set(mode).is_ok() || self::mode() == mode
}

/// The program's name as people see it: window title, menus.
pub fn app_name() -> &'static str {
    match mode() {
        // On Linux the laptop app has always been plain "n0te".
        Mode::Laptop if cfg!(target_os = "linux") => "n0te",
        Mode::Laptop => "n0te Laptop",
        Mode::Tablet => "n0te Tablet",
    }
}

/// Runs the laptop app in a window. `import`: a `.n0te` file to import first (from the command line).
pub fn run(import: Option<std::path::PathBuf>) -> iced::Result {
    run_as(Mode::Laptop, import)
}

/// Runs the app as `mode` (see [`Mode`]).
pub fn run_as(mode: Mode, import: Option<std::path::PathBuf>) -> iced::Result {
    set_mode(mode);
    let root = app::default_root();
    let mut app = iced::application(move || App::boot_with(root.clone(), import.clone()), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(iced::window::Settings { icon: window_icon(mode), ..Default::default() })
        .window_size((1440.0, 900.0))
        .antialiasing(true)
        .default_font(theme::SANS);
    for font in theme::FONT_DATA {
        app = app.font(font);
    }
    let r = app.run();
    // Pages still being stored go to disk before the program ends.
    saver::flush();
    r
}

/// The program's own icon for its windows (Windows, X11; Wayland takes it from the .desktop
/// file): the white page for n0te Laptop, the red nib for n0te Tablet. Rendered from
/// packaging/icons/*.svg by packaging/icons/render.sh.
pub fn window_icon(mode: Mode) -> Option<iced::window::Icon> {
    let png: &[u8] = match mode {
        Mode::Laptop => include_bytes!("../../../packaging/icons/n0te-laptop.png"),
        Mode::Tablet => include_bytes!("../../../packaging/icons/n0te-tablet.png"),
    };
    iced::window::icon::from_file_data(png, None).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_programs_have_a_window_icon() {
        assert!(window_icon(Mode::Laptop).is_some());
        assert!(window_icon(Mode::Tablet).is_some());
    }
}
