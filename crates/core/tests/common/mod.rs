//! Shared by the integration tests: a seeded random generator (the same cases on every run,
//! no dependency) and temporary libraries that clean up after themselves.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// SplitMix64: small, fast, good enough to pick test cases.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// 0..n (n > 0).
    pub fn below(&mut self, n: usize) -> usize {
        (self.u64() % n as u64) as usize
    }
    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent as usize
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
    /// A float in lo..hi.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (self.u64() >> 40) as f32 / (1u64 << 24) as f32 * (hi - lo)
    }
    /// Mostly ordinary values, sometimes the ones that break geometry: NaN, ±∞, huge, tiny, 0.
    pub fn nasty_f32(&mut self) -> f32 {
        match self.below(12) {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            3 => f32::MAX,
            4 => -1e30,
            5 => 0.0,
            6 => f32::MIN_POSITIVE,
            7 => -0.0,
            _ => self.range(-2000.0, 2000.0),
        }
    }
    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.u64() as u8).collect()
    }
}

/// Turns a hang into a failure that names the case: `case` records what runs now; when one
/// case takes longer than `limit`, the process ends with it printed (a loop that never ends
/// would otherwise just keep the test running).
pub struct Watchdog {
    state: std::sync::Arc<std::sync::Mutex<(std::time::Instant, String, bool)>>,
}

impl Watchdog {
    pub fn new(limit: std::time::Duration) -> Self {
        let state = std::sync::Arc::new(std::sync::Mutex::new((std::time::Instant::now(), String::new(), false)));
        let s = state.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let g = s.lock().unwrap_or_else(|p| p.into_inner());
            if g.2 {
                return;
            }
            if g.0.elapsed() > limit {
                // Straight to stderr: the test harness captures eprintln! (also from this thread).
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "stuck for more than {limit:?} on: {}", g.1);
                std::process::exit(101);
            }
        });
        Watchdog { state }
    }
    pub fn case(&self, what: impl Into<String>) {
        let mut g = self.state.lock().unwrap_or_else(|p| p.into_inner());
        *g = (std::time::Instant::now(), what.into(), false);
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).2 = true;
    }
}

/// A library in its own temporary directory, removed when dropped.
pub struct TempLib {
    pub dir: PathBuf,
}

impl TempLib {
    pub fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("n0te-t-{tag}-{}-{}", std::process::id(), n0te_core::ids::new_id(77)));
        std::fs::create_dir_all(&dir).unwrap();
        TempLib { dir }
    }
    pub fn path(&self) -> &Path {
        &self.dir
    }
    pub fn open(&self) -> n0te_core::store::Library {
        n0te_core::store::Library::open(&self.dir).unwrap()
    }
}

impl Drop for TempLib {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
