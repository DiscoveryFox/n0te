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
    /// Guests' frames (where they look); `remote_frame` covers own devices.
    pub guest_frames: bool,
    /// Guests see where this laptop looks.
    pub share_view: bool,
    /// Display formulas ($$…$$, formula blocks) centred.
    pub formula_center: bool,
    /// Touchpad: the page glides on after a two-finger swipe (stops when fingers rest again).
    pub momentum: bool,
    /// Page size (pt) for new notebooks.
    pub new_w: f32,
    pub new_h: f32,
}

/// The current `formula_center` (read while drawing text; set from the settings).
pub static FORMULA_CENTER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn formula_center() -> bool {
    FORMULA_CENTER.load(std::sync::atomic::Ordering::Relaxed)
}

impl Default for Settings {
    fn default() -> Self {
        let o = Options::default();
        Settings {
            theme: "nothing".into(),
            row_gap: o.row_gap,
            side_gap: o.side_gap,
            remote_frame: true,
            formula_center: true,
            guest_frames: true,
            share_view: true,
            momentum: true,
            new_w: 595.0,
            new_h: 842.0,
        }
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
                "formula_center" => s.formula_center = v != "0",
                "guest_frames" => s.guest_frames = v != "0",
                "share_view" => s.share_view = v != "0",
                "momentum" => s.momentum = v != "0",
                "new_w" => s.new_w = v.parse().unwrap_or(s.new_w).clamp(n0te_core::format::MIN_PT, n0te_core::format::MAX_PT),
                "new_h" => s.new_h = v.parse().unwrap_or(s.new_h).clamp(n0te_core::format::MIN_PT, n0te_core::format::MAX_PT),
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
        let _ = std::fs::write(p, self.text());
    }

    /// The settings file's text (what [`Settings::parse`] reads back).
    pub fn text(&self) -> String {
        format!(
            "theme={}\nrow_gap={}\nside_gap={}\nremote_frame={}\nformula_center={}\nguest_frames={}\nshare_view={}\nmomentum={}\nnew_w={}\nnew_h={}\n",
            self.theme,
            self.row_gap,
            self.side_gap,
            if self.remote_frame { 1 } else { 0 },
            if self.formula_center { 1 } else { 0 },
            if self.guest_frames { 1 } else { 0 },
            if self.share_view { 1 } else { 0 },
            if self.momentum { 1 } else { 0 },
            self.new_w,
            self.new_h
        )
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

    #[test]
    fn guest_switches_default_on_and_survive_a_save() {
        let d = Settings::default();
        assert!(d.guest_frames && d.share_view);
        let s = Settings { guest_frames: false, ..Settings::default() };
        let back = Settings::parse(&s.text());
        assert_eq!(back, s);
        assert!(!back.guest_frames && back.share_view);
    }

    #[test]
    fn momentum_and_the_new_notebook_size_survive_a_save() {
        let s = Settings { momentum: false, new_w: 842.0, new_h: 595.0, ..Settings::default() };
        assert_eq!(Settings::parse(&s.text()), s);
        assert!(Settings::default().momentum);
        assert_eq!(Settings::parse("new_w=1\n").new_w, n0te_core::format::MIN_PT, "clamped");
    }
}
