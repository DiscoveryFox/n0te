//! Laptop settings: theme and page gaps, how the library and the outline look, folders made
//! here that are still empty, when notebooks were opened, hints already shown — in
//! $XDG_CONFIG_HOME/n0te/laptop.conf (key=value).

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
    /// The library as a list (else cards).
    pub lib_list: bool,
    /// Folders made with "Neuer Ordner" that hold no notebook yet (they live in no document).
    pub lib_folders: Vec<String>,
    /// When notebooks were opened here: (id, unix ms), newest first, at most [`OPENED_KEPT`].
    pub lib_opened: Vec<(String, u64)>,
    /// The outline as page pictures (else the list).
    pub outline_pictures: bool,
    /// The hint on how two notebooks side by side work was shown once.
    pub split_seen: bool,
}

/// How many open times the settings keep.
pub const OPENED_KEPT: usize = 40;

/// Separates folder paths in the settings line (no folder name holds it).
const FOLDER_SEP: char = '\u{1f}';

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
            lib_list: false,
            lib_folders: Vec::new(),
            lib_opened: Vec::new(),
            outline_pictures: false,
            split_seen: false,
        }
    }
}

/// The settings file: `$XDG_CONFIG_HOME/n0te/laptop.conf`; Windows
/// `%APPDATA%\n0te\config\laptop.conf`.
pub fn path() -> PathBuf {
    n0te_platform::paths::config_dir().join("laptop.conf")
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
                "lib_view" => s.lib_list = v == "list",
                "lib_folders" => s.lib_folders = v.split(FOLDER_SEP).map(n0te_core::notebook::folder_path).filter(|f| !f.is_empty()).collect(),
                // "id:ms;id:ms", like the tablet keeps them.
                "lib_opened" => {
                    s.lib_opened = v.split(';').filter_map(|p| p.split_once(':')).filter_map(|(id, t)| Some((id.to_string(), t.parse().ok()?))).collect();
                    s.lib_opened.sort_by(|a, b| b.1.cmp(&a.1));
                    s.lib_opened.truncate(OPENED_KEPT);
                }
                "outline_view" => s.outline_pictures = v == "pictures",
                "split_seen" => s.split_seen = v == "1",
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

    /// Notes that notebook `id` was opened now (unix ms `at`): newest first, the oldest beyond
    /// [`OPENED_KEPT`] forgotten.
    pub fn opened_now(&mut self, id: &str, at: u64) {
        self.lib_opened.retain(|(i, _)| i != id);
        self.lib_opened.insert(0, (id.to_string(), at));
        self.lib_opened.sort_by(|a, b| b.1.cmp(&a.1));
        self.lib_opened.truncate(OPENED_KEPT);
    }

    /// The settings file's text (what [`Settings::parse`] reads back).
    pub fn text(&self) -> String {
        let folders: Vec<&str> = self.lib_folders.iter().map(String::as_str).collect();
        let opened: Vec<String> = self.lib_opened.iter().map(|(id, t)| format!("{id}:{t}")).collect();
        format!(
            "theme={}\nrow_gap={}\nside_gap={}\nremote_frame={}\nformula_center={}\nguest_frames={}\nshare_view={}\nmomentum={}\nnew_w={}\nnew_h={}\nlib_view={}\nlib_folders={}\nlib_opened={}\noutline_view={}\nsplit_seen={}\n",
            self.theme,
            self.row_gap,
            self.side_gap,
            if self.remote_frame { 1 } else { 0 },
            if self.formula_center { 1 } else { 0 },
            if self.guest_frames { 1 } else { 0 },
            if self.share_view { 1 } else { 0 },
            if self.momentum { 1 } else { 0 },
            self.new_w,
            self.new_h,
            if self.lib_list { "list" } else { "grid" },
            folders.join(&FOLDER_SEP.to_string()),
            opened.join(";"),
            if self.outline_pictures { "pictures" } else { "list" },
            if self.split_seen { 1 } else { 0 },
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
    fn the_library_view_empty_folders_and_open_times_survive_a_save() {
        let mut s = Settings { lib_list: true, outline_pictures: true, split_seen: true, lib_folders: vec!["Uni".into(), "Uni/WS26 = Winter".into()], ..Settings::default() };
        s.opened_now("a", 10);
        s.opened_now("b", 30);
        s.opened_now("a", 50);
        assert_eq!(s.lib_opened, vec![("a".to_string(), 50), ("b".to_string(), 30)], "newest first, each once");
        assert_eq!(Settings::parse(&s.text()), s);
        for i in 0..60 {
            s.opened_now(&format!("n{i}"), 100 + i);
        }
        assert_eq!(s.lib_opened.len(), OPENED_KEPT);
        assert_eq!(s.lib_opened[0].0, "n59");
        let d = Settings::default();
        assert!(!d.lib_list && !d.outline_pictures && d.lib_folders.is_empty());
    }

    #[test]
    fn momentum_and_the_new_notebook_size_survive_a_save() {
        let s = Settings { momentum: false, new_w: 842.0, new_h: 595.0, ..Settings::default() };
        assert_eq!(Settings::parse(&s.text()), s);
        assert!(Settings::default().momentum);
        assert_eq!(Settings::parse("new_w=1\n").new_w, n0te_core::format::MIN_PT, "clamped");
    }
}
