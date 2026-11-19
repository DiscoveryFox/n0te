//! `n0te [notebook.n0te]` — opens the library; a `.n0te` file is imported (merged) and opened.
fn main() -> iced::Result {
    let import = std::env::args_os().nth(1).map(std::path::PathBuf::from).filter(|p| p.extension().is_some_and(|x| x == "n0te"));
    n0te_desktop::run(import)
}
