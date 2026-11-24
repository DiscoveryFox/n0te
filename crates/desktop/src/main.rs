//! `n0te [notebook.n0te]` — n0te Laptop: opens the library; a `.n0te` file is imported (merged)
//! and opened. On Windows this binary ships as n0te-laptop.exe.
#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() -> iced::Result {
    // A downloaded update installs before the window opens (the installer starts us again).
    if n0te_desktop::updater::apply_pending("laptop") {
        return Ok(());
    }
    let arg = std::env::args_os().nth(1);
    if let Some(link) = arg.as_ref().and_then(|a| a.to_str()) {
        n0te_desktop::set_start_link(link);
    }
    let import = arg.map(std::path::PathBuf::from).filter(|p| p.extension().is_some_and(|x| x == "n0te"));
    n0te_desktop::run(import)
}
