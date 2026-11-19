//! Laptop settings: theme and page gaps, in $XDG_CONFIG_HOME/n0te/laptop.conf (key=value).

use std::path::PathBuf;

use n0te_core::layout::Options;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub theme: String,
    pub row_gap: f32,
    pub side_gap: f32,
    /// Show the other device's view as a red frame (live).
    pub remote_frame: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let o = Options::default();
        Settings { theme: "nothing".into(), row_gap: o.row_gap, side_gap: o.side_gap, remote_frame: true }
    }
}

pub fn path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("n0te/laptop.conf")
}

impl Settings {
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "theme" => s.theme = v.to_string(),
                "row_gap" => s.row_gap = v.parse().unwrap_or(s.row_gap).clamp(4.0, 200.0),
                "side_gap" => s.side_gap = v.parse().unwrap_or(s.side_gap).clamp(4.0, 200.0),
                "remote_frame" => s.remote_frame = v != "0",
                _ => {}
            }
        }
        s
    }

    pub fn load() -> Settings {
        std::fs::read_to_string(path()).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) {
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, format!("theme={}\nrow_gap={}\nside_gap={}\nremote_frame={}\n", self.theme, self.row_gap, self.side_gap, if self.remote_frame { 1 } else { 0 }));
    }

    pub fn layout(&self) -> Options {
        Options { row_gap: self.row_gap, side_gap: self.side_gap }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_clamps() {
        let s = Settings::parse("theme=sepia\nrow_gap=60\nside_gap=9999\njunk\n");
        assert_eq!((s.theme.as_str(), s.row_gap, s.side_gap), ("sepia", 60.0, 200.0));
        assert_eq!(Settings::parse(""), Settings::default());
    }
}
