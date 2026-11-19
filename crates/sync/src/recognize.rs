//! Handwriting → LaTeX for the tablet (the lasso's "LaTeX" button). The laptop has the
//! recognizers: first n0bridge's configured engine (`n0bridge latex --test`: local Pix2Text or
//! Claude, as the user set it for sketches), else n0bridge's local Pix2Text worker directly
//! (offline; started once and kept warm). Returns (LaTeX, engine).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub fn recognize(png: &[u8]) -> Result<(String, String), String> {
    let via_n0bridge = n0bridge(png);
    match via_n0bridge {
        Ok(r) => Ok(r),
        Err(e1) => match local(png) {
            Ok(latex) => Ok((latex, format!("lokal (Pix2Text) · n0bridge: {}", short(&e1)))),
            Err(e2) => Err(format!("n0bridge: {e1}; lokal: {e2}")),
        },
    }
}

/// The first line of a message, at most about 60 characters.
fn short(s: &str) -> String {
    let line = s.lines().next().unwrap_or_default();
    let line = line.split(" (").next().unwrap_or(line);
    if line.chars().count() <= 60 { line.to_string() } else { format!("{}…", line.chars().take(58).collect::<String>()) }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// The configured n0bridge engine (the daemon decides: local or Claude).
fn n0bridge(png: &[u8]) -> Result<(String, String), String> {
    let bin = [home().join(".local/bin/n0bridge"), PathBuf::from("/usr/local/bin/n0bridge"), PathBuf::from("/usr/bin/n0bridge")]
        .into_iter()
        .find(|p| p.is_file())
        .ok_or("n0bridge nicht installiert")?;
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = dir.join(format!("n0te-latex-{}-{n}.png", std::process::id()));
    write_private(&path, png).map_err(|e| e.to_string())?;
    let out = run_with_timeout(Command::new(bin).args(["--json", "latex", "--test"]).arg(&path), Duration::from_secs(90));
    let _ = std::fs::remove_file(&path);
    let out = out?;
    let v: serde_json::Value = serde_json::from_str(out.trim()).map_err(|_| format!("unerwartete Antwort: {}", out.trim()))?;
    if v["ok"].as_bool() == Some(true) {
        let latex = v["message"].as_str().unwrap_or_default().trim().to_string();
        let engine = v["text"].as_str().unwrap_or("n0bridge").to_string();
        if latex.is_empty() {
            return Err("nichts erkannt".into());
        }
        Ok((latex, engine))
    } else {
        Err(v["error"].as_str().unwrap_or("Fehler").to_string())
    }
}

fn run_with_timeout(cmd: &mut Command, limit: Duration) -> Result<String, String> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Zeitüberschreitung".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let mut s = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = std::io::Read::read_to_string(&mut o, &mut s);
    }
    Ok(s)
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);

/// n0bridge's local Pix2Text worker (JSON lines), started on first use and kept warm.
fn local(png: &[u8]) -> Result<String, String> {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("n0bridge/latex");
    let python = base.join("venv/bin/python");
    if !python.is_file() {
        return Err("nicht installiert (n0bridge latex --install)".into());
    }
    let mut guard = WORKER.lock().map_err(|_| "Sperre")?;
    for attempt in 0..2 {
        if guard.is_none() {
            let mut child = Command::new(&python)
                .arg("-u")
                .arg(base.join("worker.py"))
                .arg("--model")
                .arg(base.join("model"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            let stdin = child.stdin.take().ok_or("kein stdin")?;
            let mut stdout = BufReader::new(child.stdout.take().ok_or("kein stdout")?);
            let mut line = String::new();
            stdout.read_line(&mut line).map_err(|e| e.to_string())?;
            if !line.contains("\"ready\"") {
                let _ = child.kill();
                return Err(format!("Modell lädt nicht: {}", line.trim()));
            }
            *guard = Some(Worker { child, stdin, stdout, next: 1 });
        }
        let w = guard.as_mut().expect("started");
        let id = w.next;
        w.next += 1;
        let req = serde_json::json!({ "id": id, "png": base64(png) }).to_string();
        let ok = writeln!(w.stdin, "{req}").and_then(|_| w.stdin.flush()).is_ok();
        let mut line = String::new();
        if ok && w.stdout.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
            let v: serde_json::Value = serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
            if let Some(latex) = v["latex"].as_str() {
                return if latex.trim().is_empty() { Err("nichts erkannt".into()) } else { Ok(latex.trim().to_string()) };
            }
            return Err(v["error"].as_str().unwrap_or("Fehler").to_string());
        }
        // The worker died: start it again once.
        if let Some(mut w) = guard.take() {
            let _ = w.child.kill();
            let _ = w.child.wait();
        }
        if attempt == 1 {
            break;
        }
    }
    Err("Worker antwortet nicht".into())
}

fn write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    o.mode(0o600);
    o.open(path)?.write_all(data)
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(super::base64(b"n0te"), "bjB0ZQ==");
        assert_eq!(super::base64(b"ab"), "YWI=");
        assert_eq!(super::base64(b"abc"), "YWJj");
    }
}
