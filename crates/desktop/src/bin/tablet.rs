//! `n0te-tablet [notebook.n0te]` — n0te Tablet: the app set up for pen and touch (the port of
//! the Android tablet app to Windows pen devices; runs on Linux too). A `.n0te` file is imported
//! (merged) and opened.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> iced::Result {
    let import = std::env::args_os().nth(1).map(std::path::PathBuf::from).filter(|p| p.extension().is_some_and(|x| x == "n0te"));
    n0te_desktop::run_as(n0te_desktop::Mode::Tablet, import)
}
