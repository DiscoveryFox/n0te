//! LaTeX formulas as vector images: the local TeX installation (`latex` + `dvisvgm`) renders
//! each formula once to SVG in `~/.cache/n0te/tex` (Windows: `%LOCALAPPDATA%\n0te\cache\tex`,
//! TeX Live or MiKTeX from PATH); the editor shows it with iced's svg widget.
//!
//! Rendering runs on a worker thread (a formula takes ~0.3 s the first time); until then
//! [`formula`] says `Pending` and the source is shown. [`subscription`] fires when one is ready.
//! TeX runs without shell escape, may only read/write its own directory (`openin_any=p`,
//! `openout_any=p`, TeX Live's settings) and is stopped after 10 s, so a shared notebook can't
//! do anything with it.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use iced::widget::svg;

#[derive(Debug, Clone)]
pub enum Formula {
    Pending,
    /// Size in TeX points (10 pt text).
    Ready { handle: svg::Handle, w: f32, h: f32 },
    Failed(String),
}

struct State {
    cache: HashMap<String, Formula>,
    /// When a pending formula was last asked for (the view asks on every redraw).
    asked: HashMap<String, Instant>,
    jobs: mpsc::Sender<(String, String, bool)>,
}

/// A pending formula nobody asked for this long is dropped (typing makes many in-between ones).
const STALE: Duration = Duration::from_millis(1500);

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static NOTIFY: Mutex<Option<iced::futures::channel::mpsc::UnboundedSender<()>>> = Mutex::new(None);

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<(String, String, bool)>();
        std::thread::Builder::new()
            .name("n0te-tex".into())
            .spawn(move || {
                let mut queue: Vec<(String, String, bool)> = Vec::new();
                loop {
                    while let Ok(job) = rx.try_recv() {
                        queue.push(job);
                    }
                    if queue.is_empty() {
                        match rx.recv() {
                            Ok(job) => queue.push(job),
                            Err(_) => return,
                        }
                        continue;
                    }
                    // The most recently asked-for formula first; stale ones are forgotten.
                    let Some(st) = STATE.get() else { return };
                    let pick = {
                        let mut g = st.lock().expect("tex");
                        let now = Instant::now();
                        let mut fresh = Vec::new();
                        for job in queue.drain(..) {
                            match g.asked.get(&job.0).copied() {
                                Some(t) if now.duration_since(t) < STALE => fresh.push((t, job)),
                                _ => {
                                    g.cache.remove(&job.0);
                                    g.asked.remove(&job.0);
                                }
                            }
                        }
                        fresh.sort_by(|a, b| b.0.cmp(&a.0));
                        let mut it = fresh.into_iter().map(|(_, j)| j);
                        let first = it.next();
                        queue.extend(it);
                        first
                    };
                    let Some((key, latex, display)) = pick else { continue };
                    let f = render(&latex, display);
                    {
                        let mut g = st.lock().expect("tex");
                        g.asked.remove(&key);
                        g.cache.insert(key, f);
                    }
                    if let Some(n) = NOTIFY.lock().expect("notify").as_ref() {
                        let _ = n.unbounded_send(());
                    }
                }
            })
            .expect("tex thread");
        Mutex::new(State { cache: HashMap::new(), asked: HashMap::new(), jobs: tx })
    })
}

/// The formula `latex` (display style: `$$…$$`), rendering it in the background if needed.
pub fn formula(latex: &str, display: bool) -> Formula {
    let latex = latex.trim();
    if latex.is_empty() {
        return Formula::Failed("leer".into());
    }
    let key = format!("{}|{latex}", display as u8);
    let mut st = state().lock().expect("tex");
    if let Some(f) = st.cache.get(&key).cloned() {
        if matches!(f, Formula::Pending) {
            st.asked.insert(key, Instant::now());
        }
        return f;
    }
    st.cache.insert(key.clone(), Formula::Pending);
    st.asked.insert(key.clone(), Instant::now());
    let _ = st.jobs.send((key, latex.to_string(), display));
    Formula::Pending
}

/// Fires whenever a formula finished rendering (redraw then).
pub fn subscription() -> iced::Subscription<()> {
    iced::Subscription::run(stream)
}

fn stream() -> iced::futures::stream::BoxStream<'static, ()> {
    use iced::futures::StreamExt;
    let (tx, rx) = iced::futures::channel::mpsc::unbounded();
    *NOTIFY.lock().expect("notify") = Some(tx);
    rx.boxed()
}

fn cache_dir() -> PathBuf {
    n0te_platform::paths::cache_dir().join("tex")
}

fn render(latex: &str, display: bool) -> Formula {
    let dir = cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return Formula::Failed("Cache-Ordner nicht beschreibbar".into());
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (latex, display, 2u8).hash(&mut h);
    let name = format!("{:016x}", h.finish());
    let svg_path = dir.join(format!("{name}.svg"));
    if let Some(f) = load(&svg_path) {
        return f;
    }
    let tex = format!(
        "\\documentclass{{article}}\\usepackage{{amsmath,amssymb}}\\usepackage[active,tightpage]{{preview}}\\pagestyle{{empty}}\n\\begin{{document}}\\begin{{preview}}${}{}$\\end{{preview}}\\end{{document}}\n",
        if display { "\\displaystyle " } else { "" },
        latex
    );
    if std::fs::write(dir.join(format!("{name}.tex")), tex).is_err() {
        return Formula::Failed("Schreiben fehlgeschlagen".into());
    }
    let result = (|| {
        run(Command::new("latex").args(["-interaction=nonstopmode", "-halt-on-error", "-no-shell-escape", &format!("{name}.tex")]), &dir)
            .map_err(|e| if e.contains("not found") || e.contains("No such file") { "LaTeX ist nicht installiert (texlive)".to_string() } else { first_error(&dir.join(format!("{name}.log"))).unwrap_or(e) })?;
        run(Command::new("dvisvgm").args(["--no-fonts", "--exact-bbox", "-o", &format!("{name}.svg"), &format!("{name}.dvi")]), &dir)?;
        // A hairline outline on every glyph: TeX's thin rules (minus, fraction bars, the bar of
        // ϵ) are ~0.4 pt and would vanish below a pixel on screen.
        let svg = std::fs::read_to_string(&svg_path).map_err(|e| e.to_string())?;
        let svg = svg.replacen("<g id='page1'>", "<g id='page1' stroke='#000' stroke-width='0.14' stroke-linejoin='round'>", 1);
        std::fs::write(&svg_path, svg).map_err(|e| e.to_string())?;
        load(&svg_path).ok_or_else(|| "keine SVG-Ausgabe".to_string())
    })();
    for ext in ["tex", "dvi", "aux", "log"] {
        let _ = std::fs::remove_file(dir.join(format!("{name}.{ext}")));
    }
    result.unwrap_or_else(Formula::Failed)
}

/// Runs a TeX tool in `dir`, confined to it, for at most 10 s (Windows: no console window).
fn run(cmd: &mut Command, dir: &Path) -> Result<(), String> {
    let mut child = n0te_platform::no_window(cmd)
        .current_dir(dir)
        .env("openin_any", "p")
        .env("openout_any", "p")
        .env("shell_escape", "f")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("Fehler ({status})")),
            Ok(None) if start.elapsed() > Duration::from_secs(10) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Zeitüberschreitung".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// The first "! …" error line of a TeX log.
fn first_error(log: &Path) -> Option<String> {
    let text = std::fs::read_to_string(log).ok()?;
    text.lines().find(|l| l.starts_with("! ")).map(|l| l.trim_start_matches("! ").to_string())
}

fn load(path: &Path) -> Option<Formula> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let (w, h) = (attr_pt(&text, "width")?, attr_pt(&text, "height")?);
    Some(Formula::Ready { handle: svg::Handle::from_memory(bytes), w, h })
}

/// A length attribute of the root `<svg>` element in points (dvisvgm writes `12.3pt`).
fn attr_pt(svg: &str, name: &str) -> Option<f32> {
    let root = &svg[svg.find("<svg")?..];
    let root = &root[..root.find('>')?];
    let at = root.find(&format!(" {name}="))? + name.len() + 2;
    let quote = root[at..].chars().next()?;
    let rest = &root[at + 1..];
    let value = &rest[..rest.find(quote)?];
    let num: String = value.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let v: f32 = num.parse().ok()?;
    Some(if value.ends_with("px") { v * 0.75 } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_svg_sizes() {
        let s = "<?xml version='1.0'?>\n<svg version='1.1' xmlns='http://www.w3.org/2000/svg' width='23.5pt' height='9.1pt' viewBox='0 0 23.5 9.1'>";
        assert_eq!(attr_pt(s, "width"), Some(23.5));
        assert_eq!(attr_pt(s, "height"), Some(9.1));
    }

    /// Renders with the local TeX (skipped where TeX Live isn't installed).
    #[test]
    fn renders_a_formula_with_local_tex() {
        if Command::new("latex").arg("--version").stdout(Stdio::null()).status().is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("n0te-tex-test-{}", std::process::id()));
        unsafe { std::env::set_var("XDG_CACHE_HOME", &dir) };
        match render("\\int_0^1 x^2\\,dx = \\frac{1}{3}", true) {
            Formula::Ready { w, h, .. } => assert!(w > 10.0 && h > 5.0, "{w}×{h}"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(render("\\frac{1}{", false), Formula::Failed(_)), "broken LaTeX is reported, not hung");
        let _ = std::fs::remove_dir_all(dir);
    }
}
