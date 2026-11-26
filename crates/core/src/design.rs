//! Designs: how n0te looks. A design is a palette (the chrome's colours and the paper's tint on
//! screen; exported PDFs stay white), a style (corner radii, how secondary controls are drawn,
//! the voice of labels, the face of headings, soft shadows) and optional decorations behind the
//! library (snow, fairy lights, stars, …).
//!
//! The built-in designs are JSON files in `crates/core/designs/`. People can write their own in
//! the same format, usually by asking an AI with [`ai_prompt`], and import them. Both apps read
//! designs from here, so a file looks the same on the tablet and on the laptop, and a file
//! written for one imports on the other.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The format version this build reads and writes (`"n0te_style": 1`).
pub const FORMAT: u64 = 1;
/// A design file is a few hundred bytes; anything far bigger is not one.
pub const MAX_BYTES: usize = 64 * 1024;
/// Longest name shown on a design card.
pub const MAX_NAME: usize = 40;
/// Longest description.
pub const MAX_DESC: usize = 120;
/// Corner radii are clamped to this (dp on the tablet, logical px on the laptop).
pub const MAX_RADIUS: f32 = 32.0;
/// At most this many decorations at once.
pub const MAX_DECOR: usize = 3;

/// The voice of button labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// "PDF IMPORTIEREN" in Space Mono.
    MonoCaps,
    /// "PDF importieren" in Space Grotesk.
    Sans,
    /// "PDF importieren" in a serif.
    Serif,
}

/// The face of headings (screen and dialog titles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heading {
    /// Doto, a dot-matrix face (written in lower case).
    Dot,
    Mono,
    Sans,
    Serif,
}

/// How secondary buttons, cards and panels are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// A 1 px outline on the background.
    Outline,
    /// A tinted surface without an outline.
    Tonal,
    /// Hairlines; hover inverts.
    Rule,
}

/// Something drawn behind the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorKind {
    /// Falling snowflakes.
    Snow,
    /// A string of fairy lights along the top edge, glowing in turn.
    Lights,
    /// Twinkling stars.
    Stars,
    /// A drafting grid.
    Grid,
    /// Falling blossom petals.
    Petals,
    /// Falling leaves.
    Leaves,
    /// The faint lines of an old screen.
    Scanlines,
}

impl DecorKind {
    pub const ALL: [DecorKind; 7] = [Self::Snow, Self::Lights, Self::Stars, Self::Grid, Self::Petals, Self::Leaves, Self::Scanlines];

    pub fn id(self) -> &'static str {
        match self {
            Self::Snow => "snow",
            Self::Lights => "lights",
            Self::Stars => "stars",
            Self::Grid => "grid",
            Self::Petals => "petals",
            Self::Leaves => "leaves",
            Self::Scanlines => "scanlines",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Some(match norm(id).as_str() {
            "snow" | "snowflakes" | "snowfall" => Self::Snow,
            "lights" | "fairy_lights" | "fairylights" | "string_lights" | "garland" => Self::Lights,
            "stars" | "starfield" | "twinkle" => Self::Stars,
            "grid" | "blueprint" | "graph_paper" => Self::Grid,
            "petals" | "blossoms" | "sakura" | "flowers" => Self::Petals,
            "leaves" | "autumn" | "foliage" => Self::Leaves,
            "scanlines" | "scan_lines" | "crt" => Self::Scanlines,
            _ => return None,
        })
    }

    /// Moves over time (the apps hold still when animations are off).
    pub fn animated(self) -> bool {
        !matches!(self, Self::Grid | Self::Scanlines)
    }
}

/// One decoration: what, in which colours (0xRRGGBB) and how much of it (0 = a little, 1 = a lot).
#[derive(Debug, Clone, PartialEq)]
pub struct Decor {
    pub kind: DecorKind,
    pub colors: Vec<u32>,
    pub amount: f32,
}

/// The colours of a design, 0xRRGGBB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colors {
    /// The window behind everything.
    pub background: u32,
    /// Cards, bars, menus, dialogs.
    pub surface: u32,
    /// A step above the surface (pressed, menus on cards, fields on light designs).
    pub surface_2: u32,
    pub border: u32,
    pub text: u32,
    /// Secondary text (hints, counts).
    pub muted: u32,
    /// The one signal colour (primary button, selection, sliders).
    pub accent: u32,
    /// Text and icons on the accent.
    pub on_accent: u32,
    /// The page's tint on screen; always light, ink is dark.
    pub paper: u32,
}

/// A whole design.
#[derive(Debug, Clone, PartialEq)]
pub struct Design {
    /// Lower case letters, digits and dashes; unique.
    pub id: String,
    /// German name and description; the English ones fall back to them.
    pub name: String,
    pub name_en: String,
    pub desc: String,
    pub desc_en: String,
    pub author: String,
    /// Dark chrome (light status bar icons, dark-mode tints).
    pub dark: bool,
    pub colors: Colors,
    /// Corner radii of buttons, cards and panels.
    pub radius: [f32; 3],
    pub label: Label,
    pub heading: Heading,
    pub fill: Fill,
    /// Soft shadows under cards and dialogs.
    pub shadows: bool,
    pub decor: Vec<Decor>,
    /// Ships with n0te (can't be deleted or replaced).
    pub builtin: bool,
}

impl Design {
    pub fn name_in(&self, english: bool) -> &str {
        if english && !self.name_en.is_empty() { &self.name_en } else { &self.name }
    }

    pub fn desc_in(&self, english: bool) -> &str {
        if english && !self.desc_en.is_empty() { &self.desc_en } else { &self.desc }
    }

    pub fn button_radius(&self) -> f32 {
        self.radius[0]
    }
    pub fn card_radius(&self) -> f32 {
        self.radius[1]
    }
    pub fn panel_radius(&self) -> f32 {
        self.radius[2]
    }

    /// Anything here moves (the apps run a frame clock only then).
    pub fn animated(&self) -> bool {
        self.decor.iter().any(|d| d.kind.animated())
    }
}

/// Something wrong (or worth knowing) about a design file, in German and English. `field` is
/// the JSON path ("colors.accent"), empty for the file as a whole.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub field: String,
    pub de: String,
    pub en: String,
}

impl Problem {
    fn new(field: &str, de: impl Into<String>, en: impl Into<String>) -> Self {
        Problem { field: field.to_string(), de: de.into(), en: en.into() }
    }

    /// "colors.accent: ist keine Farbe" in the chosen language.
    pub fn text(&self, english: bool) -> String {
        let m = if english { &self.en } else { &self.de };
        if self.field.is_empty() { m.clone() } else { format!("{}: {m}", self.field) }
    }
}

/// A design read from a file, with what was fixed up or looks odd.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub design: Design,
    pub warnings: Vec<Problem>,
}

// --- colour maths ------------------------------------------------------------------------------

fn channels(c: u32) -> [f32; 3] {
    [((c >> 16) & 0xFF) as f32 / 255.0, ((c >> 8) & 0xFF) as f32 / 255.0, (c & 0xFF) as f32 / 255.0]
}

/// WCAG relative luminance (0 black … 1 white).
pub fn luminance(c: u32) -> f32 {
    let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    let [r, g, b] = channels(c);
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG contrast ratio (1 … 21); 4.5 is comfortable body text, 3 large text.
pub fn contrast(a: u32, b: u32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `a` moved towards `b` by `t` (0 … 1), per channel in sRGB.
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let (ca, cb) = (channels(a), channels(b));
    let ch = |i: usize| ((ca[i] + (cb[i] - ca[i]) * t).clamp(0.0, 1.0) * 255.0).round() as u32;
    (ch(0) << 16) | (ch(1) << 8) | ch(2)
}

/// "#RGB", "#RRGGBB", "#RRGGBBAA" (alpha ignored), with or without "#", or "rgb(r, g, b)".
pub fn parse_color(s: &str) -> Option<u32> {
    let t = s.trim();
    if let Some(inner) = t.strip_prefix("rgb(").or_else(|| t.strip_prefix("RGB(")).and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<u32> = inner.split(',').map(|p| p.trim().parse::<u32>().ok().filter(|v| *v <= 255)).collect::<Option<_>>()?;
        return (parts.len() == 3).then(|| (parts[0] << 16) | (parts[1] << 8) | parts[2]);
    }
    let h = t.strip_prefix('#').unwrap_or(t);
    if h.is_empty() || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match h.len() {
        3 => {
            let v = u32::from_str_radix(h, 16).ok()?;
            let (r, g, b) = ((v >> 8) & 0xF, (v >> 4) & 0xF, v & 0xF);
            Some((r * 17) << 16 | (g * 17) << 8 | b * 17)
        }
        6 => u32::from_str_radix(h, 16).ok(),
        8 => u32::from_str_radix(&h[..6], 16).ok(),
        _ => None,
    }
}

/// "#1a2b3c".
pub fn hex(c: u32) -> String {
    format!("#{:06x}", c & 0xFF_FFFF)
}

// --- reading -----------------------------------------------------------------------------------

/// Lower case, dashes and spaces as underscores: "Mono-Caps" → "mono_caps".
fn norm(s: &str) -> String {
    s.trim().to_lowercase().replace(['-', ' '], "_")
}

/// The JSON object in `text`. AI answers wrap it in prose and ``` fences and sometimes add
/// comments, trailing commas or typographic quotes; all of that is tolerated.
fn extract(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    let body = &text[start..=end];
    let cleaned = strip_comments_and_commas(body);
    serde_json::from_str::<Value>(&cleaned).ok().or_else(|| {
        let straight = cleaned.replace(['\u{201C}', '\u{201D}', '\u{201E}', '\u{00AB}', '\u{00BB}'], "\"");
        serde_json::from_str::<Value>(&strip_comments_and_commas(&straight)).ok()
    })
}

/// Removes `//` and `/* */` comments and commas before `}` or `]`, outside strings.
fn strip_comments_and_commas(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let mut in_str = false;
    while i < c.len() {
        let ch = c[i];
        if in_str {
            out.push(ch);
            if ch == '\\' && i + 1 < c.len() {
                out.push(c[i + 1]);
                i += 2;
                continue;
            }
            if ch == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '"' => {
                in_str = true;
                out.push(ch);
            }
            '/' if c.get(i + 1) == Some(&'/') => {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '/' if c.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < c.len() && !(c[i] == '*' && c[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            ',' => {
                let mut j = i + 1;
                while j < c.len() && c[j].is_whitespace() {
                    j += 1;
                }
                if !matches!(c.get(j), Some('}') | Some(']')) {
                    out.push(ch);
                }
            }
            _ => out.push(ch),
        }
        i += 1;
    }
    out
}

/// The closest of `known` to `word` if it is a plausible typo ("acent" → "accent").
fn suggest<'a>(word: &str, known: &[&'a str]) -> Option<&'a str> {
    fn dist(a: &str, b: &str) -> usize {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.iter().enumerate() {
            let mut cur = vec![i + 1; b.len() + 1];
            for (j, cb) in b.iter().enumerate() {
                cur[j + 1] = (prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1);
            }
            prev = cur;
        }
        prev[b.len()]
    }
    let w = norm(word);
    known.iter().map(|k| (dist(&w, k), *k)).filter(|(d, k)| *d <= 2.max(k.len() / 4)).min_by_key(|(d, _)| *d).map(|(_, k)| k)
}

struct Reader {
    errors: Vec<Problem>,
    warnings: Vec<Problem>,
}

impl Reader {
    fn err(&mut self, field: &str, de: impl Into<String>, en: impl Into<String>) {
        self.errors.push(Problem::new(field, de, en));
    }
    fn warn(&mut self, field: &str, de: impl Into<String>, en: impl Into<String>) {
        self.warnings.push(Problem::new(field, de, en));
    }

    fn unknown_keys(&mut self, obj: &Map<String, Value>, known: &[&str], path: &str) {
        for k in obj.keys() {
            if known.contains(&norm(k).as_str()) {
                continue;
            }
            let field = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
            match suggest(k, known) {
                Some(s) => self.warn(&field, format!("unbekannt, ignoriert (gemeint: \"{s}\"?)"), format!("unknown, ignored (did you mean \"{s}\"?)")),
                None => self.warn(&field, "unbekannt, ignoriert", "unknown, ignored"),
            }
        }
    }

    fn text(&mut self, obj: &Map<String, Value>, key: &str, max: usize) -> Option<String> {
        let v = get(obj, &[key])?;
        let Some(s) = v.as_str() else {
            self.err(key, "muss Text sein", "must be text");
            return None;
        };
        let s: String = s.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string();
        if s.chars().count() > max {
            self.warn(key, format!("auf {max} Zeichen gekürzt"), format!("shortened to {max} characters"));
            return Some(s.chars().take(max).collect::<String>().trim_end().to_string());
        }
        Some(s)
    }

    fn color(&mut self, colors: &Map<String, Value>, keys: &[&str]) -> Option<u32> {
        let v = get(colors, keys)?;
        let field = format!("colors.{}", keys[0]);
        match v.as_str().and_then(parse_color) {
            Some(c) => {
                if v.as_str().is_some_and(|s| s.trim().trim_start_matches('#').len() == 8) {
                    self.warn(&field, "Transparenz wird ignoriert", "transparency is ignored");
                }
                Some(c)
            }
            None => {
                self.err(&field, format!("{} ist keine Farbe (Beispiel: \"#1E5BD8\")", v), format!("{} is not a colour (example: \"#1E5BD8\")", v));
                None
            }
        }
    }

    fn choice<T: Copy>(&mut self, obj: &Map<String, Value>, key: &str, options: &[(&[&str], T)], allowed: &str) -> Option<T> {
        let v = get(obj, &[key])?;
        let s = v.as_str().map(norm).unwrap_or_default();
        for (names, value) in options {
            if names.contains(&s.as_str()) {
                return Some(*value);
            }
        }
        self.err(key, format!("{v} gibt es nicht; erlaubt: {allowed}"), format!("{v} doesn't exist; allowed: {allowed}"));
        None
    }
}

/// `obj[k]` for the first of `keys` (aliases) present, by normalized name.
fn get<'a>(obj: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    obj.iter().find(|(k, _)| keys.contains(&norm(k).as_str())).map(|(_, v)| v)
}

/// "Mein Design!" → "mein-design".
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        let ch = match ch {
            'ä' | 'Ä' => "ae",
            'ö' | 'Ö' => "oe",
            'ü' | 'Ü' => "ue",
            'ß' => "ss",
            c if c.is_ascii_alphanumeric() => {
                out.push(c.to_ascii_lowercase());
                continue;
            }
            _ => "-",
        };
        if ch == "-" {
            if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
        } else {
            out.push_str(ch);
        }
    }
    let out = out.trim_matches('-');
    out.chars().take(40).collect::<String>().trim_end_matches('-').to_string()
}

fn is_slug(id: &str) -> bool {
    !id.is_empty() && id.len() <= 48 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

const TOP_KEYS: &[&str] = &[
    "n0te_style", "id", "name", "name_en", "description", "description_en", "desc", "desc_en", "author", "dark", "colors", "colours", "corners",
    "radius", "labels", "headings", "fill", "shadows", "decorations", "decoration", "$schema",
];
const COLOR_KEYS: &[&str] = &[
    "background", "bg", "surface", "surface_2", "surface2", "elevated", "border", "text", "foreground", "fg", "muted", "accent", "on_accent", "onaccent", "paper",
];

/// Reads a design from `text` (a file, or an AI's whole answer pasted in). On success, the
/// design plus warnings about what was adjusted; otherwise every error found at once, so an
/// AI can fix them all in one go.
pub fn parse(text: &str) -> Result<Parsed, Vec<Problem>> {
    if text.len() > MAX_BYTES {
        return Err(vec![Problem::new("", "zu groß für eine Design-Datei (höchstens 64 KB)", "too big for a design file (64 KB at most)")]);
    }
    let Some(value) = extract(text) else {
        return Err(vec![Problem::new(
            "",
            "kein gültiges JSON-Objekt gefunden (erwartet: { \"n0te_style\": 1, \"name\": …, \"colors\": { … } })",
            "no valid JSON object found (expected: { \"n0te_style\": 1, \"name\": …, \"colors\": { … } })",
        )]);
    };
    let Value::Object(obj) = value else {
        return Err(vec![Problem::new("", "kein JSON-Objekt", "not a JSON object")]);
    };
    let mut r = Reader { errors: Vec::new(), warnings: Vec::new() };
    r.unknown_keys(&obj, TOP_KEYS, "");

    match get(&obj, &["n0te_style"]) {
        None => r.warn("n0te_style", "fehlt, Version 1 angenommen", "missing, assuming version 1"),
        Some(v) => match v.as_u64() {
            Some(1) => {}
            Some(n) if n > FORMAT => r.err("n0te_style", format!("Version {n} ist neuer als diese n0te-Version (liest {FORMAT}); bitte n0te aktualisieren"), format!("version {n} is newer than this n0te reads ({FORMAT}); please update n0te")),
            _ => r.err("n0te_style", "muss 1 sein", "must be 1"),
        },
    }

    let name = r.text(&obj, "name", MAX_NAME).unwrap_or_default();
    if name.is_empty() {
        r.err("name", "fehlt (jedes Design braucht einen Namen)", "missing (every design needs a name)");
    }
    let name_en = r.text(&obj, "name_en", MAX_NAME).unwrap_or_default();
    let desc = r.text(&obj, "description", MAX_DESC).or_else(|| r.text(&obj, "desc", MAX_DESC)).unwrap_or_default();
    let desc_en = r.text(&obj, "description_en", MAX_DESC).or_else(|| r.text(&obj, "desc_en", MAX_DESC)).unwrap_or_default();
    let author = r.text(&obj, "author", MAX_NAME).unwrap_or_default();

    // Colours: four are needed, the rest follow from them.
    let colors = match get(&obj, &["colors", "colours"]) {
        Some(Value::Object(c)) => {
            r.unknown_keys(c, COLOR_KEYS, "colors");
            Some(c.clone())
        }
        Some(_) => {
            r.err("colors", "muss ein Objekt sein: { \"background\": \"#…\", … }", "must be an object: { \"background\": \"#…\", … }");
            None
        }
        None => {
            r.err("colors", "fehlt (mindestens background, surface, text, accent)", "missing (at least background, surface, text, accent)");
            None
        }
    };
    let c = colors.unwrap_or_default();
    let background = r.color(&c, &["background", "bg"]);
    let surface = r.color(&c, &["surface"]);
    let text_c = r.color(&c, &["text", "foreground", "fg"]);
    let accent = r.color(&c, &["accent"]);
    let surface_2 = r.color(&c, &["surface_2", "surface2", "elevated"]);
    let border = r.color(&c, &["border"]);
    let muted = r.color(&c, &["muted"]);
    let on_accent = r.color(&c, &["on_accent", "onaccent"]);
    let paper = r.color(&c, &["paper"]);
    if !c.is_empty() {
        for (keys, v) in [(&["background", "bg"][..], background), (&["surface"][..], surface), (&["text", "foreground", "fg"][..], text_c), (&["accent"][..], accent)] {
            if v.is_none() && get(&c, keys).is_none() {
                r.err(&format!("colors.{}", keys[0]), "fehlt", "missing");
            }
        }
    }

    // Style.
    let radius = match get(&obj, &["corners", "radius"]) {
        None => [12.0, 16.0, 20.0],
        Some(Value::Number(n)) => {
            let v = n.as_f64().unwrap_or(12.0) as f32;
            [v; 3]
        }
        Some(Value::Object(o)) => {
            r.unknown_keys(o, &["buttons", "button", "cards", "card", "panels", "panel"], "corners");
            let mut num = |keys: &[&str], default: f32| match get(o, keys) {
                None => default,
                Some(v) => v.as_f64().map(|f| f as f32).unwrap_or_else(|| {
                    r.err(&format!("corners.{}", keys[0]), "muss eine Zahl sein", "must be a number");
                    default
                }),
            };
            [num(&["buttons", "button"], 12.0), num(&["cards", "card"], 16.0), num(&["panels", "panel"], 20.0)]
        }
        Some(_) => {
            r.err("corners", "muss eine Zahl oder { \"buttons\", \"cards\", \"panels\" } sein", "must be a number or { \"buttons\", \"cards\", \"panels\" }");
            [12.0, 16.0, 20.0]
        }
    };
    let mut radius = radius;
    for (i, which) in ["buttons", "cards", "panels"].iter().enumerate() {
        let v = radius[i];
        if !v.is_finite() || !(0.0..=MAX_RADIUS).contains(&v) {
            let clamped = if v.is_finite() { v.clamp(0.0, MAX_RADIUS) } else { 12.0 };
            r.warn(&format!("corners.{which}"), format!("{v} liegt außerhalb von 0 … {MAX_RADIUS}, nehme {clamped}"), format!("{v} is outside 0 … {MAX_RADIUS}, using {clamped}"));
            radius[i] = clamped;
        }
    }
    let label = r
        .choice(&obj, "labels", &[(&["mono_caps", "monocaps", "mono", "caps"], Label::MonoCaps), (&["sans", "sans_serif", "grotesk"], Label::Sans), (&["serif"], Label::Serif)], "\"mono_caps\", \"sans\", \"serif\"")
        .unwrap_or(Label::Sans);
    let heading = r
        .choice(&obj, "headings", &[(&["dot", "dot_matrix", "dotmatrix", "pixel"], Heading::Dot), (&["mono", "monospace"], Heading::Mono), (&["sans", "sans_serif"], Heading::Sans), (&["serif"], Heading::Serif)], "\"dot\", \"mono\", \"sans\", \"serif\"")
        .unwrap_or(Heading::Sans);
    let fill = r
        .choice(&obj, "fill", &[(&["outline", "outlined", "lines"], Fill::Outline), (&["tonal", "filled", "soft"], Fill::Tonal), (&["rule", "hairline", "hairlines"], Fill::Rule)], "\"outline\", \"tonal\", \"rule\"")
        .unwrap_or(Fill::Tonal);
    let shadows = match get(&obj, &["shadows"]) {
        None => false,
        Some(v) => v.as_bool().unwrap_or_else(|| {
            r.err("shadows", "muss true oder false sein", "must be true or false");
            false
        }),
    };
    let decor = read_decor(&mut r, &obj);

    // Stop here if the basics are broken: the checks below need the colours.
    let (Some(background), Some(surface), Some(text_c), Some(accent)) = (background, surface, text_c, accent) else {
        return Err(r.errors);
    };
    if !r.errors.is_empty() {
        return Err(r.errors);
    }

    let dark_by_colour = luminance(background) < 0.35;
    let dark = match get(&obj, &["dark"]) {
        None => dark_by_colour,
        Some(v) => match v.as_bool() {
            Some(d) => {
                if d != dark_by_colour {
                    r.warn("dark", if d { "ist true, aber der Hintergrund ist hell" } else { "ist false, aber der Hintergrund ist dunkel" }, if d { "is true but the background is light" } else { "is false but the background is dark" });
                }
                d
            }
            None => {
                r.err("dark", "muss true oder false sein", "must be true or false");
                dark_by_colour
            }
        },
    };
    let surface_2 = surface_2.unwrap_or_else(|| mix(surface, text_c, 0.07));
    let border = border.unwrap_or_else(|| mix(surface, text_c, 0.16));
    let muted = muted.unwrap_or_else(|| mix(text_c, background, 0.45));
    let on_accent = on_accent.unwrap_or(if contrast(0xFFFFFF, accent) >= contrast(0x111111, accent) { 0xFFFFFF } else { 0x111111 });
    let paper = paper.unwrap_or(0xFFFFFF);

    // Legibility.
    let ratio = contrast(text_c, background);
    if ratio < 3.0 {
        r.err("colors.text", format!("auf dem Hintergrund kaum lesbar (Kontrast {ratio:.1}:1, mindestens 4,5:1)"), format!("hard to read on the background (contrast {ratio:.1}:1, at least 4.5:1)"));
    } else if ratio < 4.5 {
        r.warn("colors.text", format!("wenig Kontrast zum Hintergrund ({ratio:.1}:1, besser 4,5:1)"), format!("low contrast with the background ({ratio:.1}:1, 4.5:1 is better)"));
    }
    let ratio = contrast(text_c, surface);
    if ratio < 3.0 {
        r.err("colors.surface", format!("Text darauf kaum lesbar (Kontrast {ratio:.1}:1, mindestens 4,5:1)"), format!("text on it is hard to read (contrast {ratio:.1}:1, at least 4.5:1)"));
    } else if ratio < 4.5 {
        r.warn("colors.surface", format!("wenig Kontrast zum Text ({ratio:.1}:1, besser 4,5:1)"), format!("low contrast with the text ({ratio:.1}:1, 4.5:1 is better)"));
    }
    if contrast(muted, background) < 2.0 {
        r.warn("colors.muted", "verschwindet fast im Hintergrund", "nearly disappears into the background");
    }
    if contrast(accent, background) < 1.6 {
        r.warn("colors.accent", "hebt sich kaum vom Hintergrund ab", "barely stands out from the background");
    }
    if contrast(on_accent, accent) < 3.0 {
        r.warn("colors.on_accent", "auf dem Akzent schlecht lesbar", "hard to read on the accent");
    }
    if luminance(paper) < 0.7 {
        r.err("colors.paper", "muss hell bleiben: Tinte ist dunkel (Helligkeit mindestens wie #DCDCDC)", "must stay light: ink is dark (at least as bright as #DCDCDC)");
    }
    if !r.errors.is_empty() {
        return Err(r.errors);
    }

    // The id: given or from the name; a built-in's id stays the built-in's.
    let wanted = get(&obj, &["id"]).and_then(Value::as_str).map(slug).filter(|s| !s.is_empty()).unwrap_or_else(|| slug(&name));
    let wanted = if wanted.is_empty() { "design".to_string() } else { wanted };
    let id = if builtin_ids().contains(&wanted.as_str()) { format!("my-{wanted}") } else { wanted };

    Ok(Parsed {
        design: Design {
            id,
            name,
            name_en,
            desc,
            desc_en,
            author,
            dark,
            colors: Colors { background, surface, surface_2, border, text: text_c, muted, accent, on_accent, paper },
            radius,
            label,
            heading,
            fill,
            shadows,
            decor,
            builtin: false,
        },
        warnings: r.warnings,
    })
}

fn read_decor(r: &mut Reader, obj: &Map<String, Value>) -> Vec<Decor> {
    let Some(v) = get(obj, &["decorations", "decoration"]) else { return Vec::new() };
    let items: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::Null => Vec::new(),
        other => vec![other],
    };
    let mut out: Vec<Decor> = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        let field = format!("decorations[{i}]");
        let (kind, colors, amount) = match item {
            Value::String(s) => (s.clone(), None, None),
            Value::Object(o) => {
                r.unknown_keys(o, &["kind", "type", "colors", "colours", "color", "amount", "density"], &field);
                let kind = get(o, &["kind", "type"]).and_then(Value::as_str).unwrap_or_default().to_string();
                (kind, get(o, &["colors", "colours", "color"]).cloned(), get(o, &["amount", "density"]).cloned())
            }
            _ => {
                r.warn(&field, "muss ein Name (\"snow\") oder ein Objekt sein, ignoriert", "must be a name (\"snow\") or an object, ignored");
                continue;
            }
        };
        if norm(&kind) == "none" || kind.is_empty() && colors.is_none() {
            continue;
        }
        let Some(k) = DecorKind::from_id(&kind) else {
            let names: Vec<&str> = DecorKind::ALL.iter().map(|k| k.id()).collect();
            r.warn(&field, format!("\"{kind}\" kennt n0te nicht, ignoriert (es gibt: {})", names.join(", ")), format!("n0te doesn't know \"{kind}\", ignored (there are: {})", names.join(", ")));
            continue;
        };
        let mut cols = Vec::new();
        match colors {
            None => {}
            Some(Value::String(s)) => match parse_color(&s) {
                Some(c) => cols.push(c),
                None => r.warn(&format!("{field}.colors"), format!("\"{s}\" ist keine Farbe, ignoriert"), format!("\"{s}\" is not a colour, ignored")),
            },
            Some(Value::Array(a)) => {
                for c in a.iter().take(8) {
                    match c.as_str().and_then(parse_color) {
                        Some(c) => cols.push(c),
                        None => r.warn(&format!("{field}.colors"), format!("{c} ist keine Farbe, ignoriert"), format!("{c} is not a colour, ignored")),
                    }
                }
            }
            Some(other) => r.warn(&format!("{field}.colors"), format!("{other} ist keine Farbliste, ignoriert"), format!("{other} is not a colour list, ignored")),
        }
        let amount = match amount {
            None => 0.5,
            Some(v) => match v.as_f64() {
                Some(a) if (0.0..=1.0).contains(&a) => a as f32,
                Some(a) if a.is_finite() => {
                    let c = a.clamp(0.0, 1.0) as f32;
                    r.warn(&format!("{field}.amount"), format!("{a} liegt außerhalb von 0 … 1, nehme {c}"), format!("{a} is outside 0 … 1, using {c}"));
                    c
                }
                _ => {
                    r.warn(&format!("{field}.amount"), "muss eine Zahl von 0 bis 1 sein, nehme 0,5", "must be a number from 0 to 1, using 0.5");
                    0.5
                }
            },
        };
        if out.iter().any(|d| d.kind == k) {
            r.warn(&field, format!("\"{}\" doppelt, ignoriert", k.id()), format!("\"{}\" twice, ignored", k.id()));
            continue;
        }
        if out.len() == MAX_DECOR {
            r.warn(&field, format!("höchstens {MAX_DECOR} Dekorationen, Rest ignoriert"), format!("{MAX_DECOR} decorations at most, the rest is ignored"));
            break;
        }
        out.push(Decor { kind: k, colors: cols, amount });
    }
    out
}

// --- writing -----------------------------------------------------------------------------------

fn label_id(l: Label) -> &'static str {
    match l {
        Label::MonoCaps => "mono_caps",
        Label::Sans => "sans",
        Label::Serif => "serif",
    }
}

fn heading_id(h: Heading) -> &'static str {
    match h {
        Heading::Dot => "dot",
        Heading::Mono => "mono",
        Heading::Sans => "sans",
        Heading::Serif => "serif",
    }
}

fn fill_id(f: Fill) -> &'static str {
    match f {
        Fill::Outline => "outline",
        Fill::Tonal => "tonal",
        Fill::Rule => "rule",
    }
}

impl Label {
    pub fn id(self) -> &'static str {
        label_id(self)
    }
}
impl Heading {
    pub fn id(self) -> &'static str {
        heading_id(self)
    }
}
impl Fill {
    pub fn id(self) -> &'static str {
        fill_id(self)
    }
}

/// The design as a file in the import format, keys in reading order (what "share" hands out
/// and what is stored for imported designs).
pub fn to_json(d: &Design) -> String {
    let q = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
    let num = |v: f32| if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v}") };
    let mut s = String::from("{\n  \"n0te_style\": 1,\n");
    s += &format!("  \"id\": {},\n  \"name\": {},\n", q(&d.id), q(&d.name));
    if !d.name_en.is_empty() {
        s += &format!("  \"name_en\": {},\n", q(&d.name_en));
    }
    if !d.desc.is_empty() {
        s += &format!("  \"description\": {},\n", q(&d.desc));
    }
    if !d.desc_en.is_empty() {
        s += &format!("  \"description_en\": {},\n", q(&d.desc_en));
    }
    if !d.author.is_empty() {
        s += &format!("  \"author\": {},\n", q(&d.author));
    }
    let c = &d.colors;
    s += &format!("  \"dark\": {},\n  \"colors\": {{\n", d.dark);
    let cols = [
        ("background", c.background), ("surface", c.surface), ("surface_2", c.surface_2), ("border", c.border), ("text", c.text), ("muted", c.muted),
        ("accent", c.accent), ("on_accent", c.on_accent), ("paper", c.paper),
    ];
    for (i, (k, v)) in cols.iter().enumerate() {
        s += &format!("    \"{k}\": \"{}\"{}\n", hex(*v), if i + 1 < cols.len() { "," } else { "" });
    }
    s += &format!(
        "  }},\n  \"corners\": {{ \"buttons\": {}, \"cards\": {}, \"panels\": {} }},\n",
        num(d.radius[0]), num(d.radius[1]), num(d.radius[2])
    );
    s += &format!("  \"labels\": \"{}\",\n  \"headings\": \"{}\",\n  \"fill\": \"{}\",\n  \"shadows\": {}", label_id(d.label), heading_id(d.heading), fill_id(d.fill), d.shadows);
    if d.decor.is_empty() {
        s += "\n}\n";
    } else {
        s += ",\n  \"decorations\": [\n";
        for (i, dec) in d.decor.iter().enumerate() {
            let cols: Vec<String> = dec.colors.iter().map(|c| format!("\"{}\"", hex(*c))).collect();
            s += &format!("    {{ \"kind\": \"{}\", \"colors\": [{}], \"amount\": {} }}{}\n", dec.kind.id(), cols.join(", "), num(dec.amount), if i + 1 < d.decor.len() { "," } else { "" });
        }
        s += "  ]\n}\n";
    }
    s
}

// --- the built-in designs ----------------------------------------------------------------------

/// The designs that ship with n0te, in the order the pickers show them: the six classics,
/// then the special ones.
const BUILTIN_FILES: [&str; 14] = [
    include_str!("../designs/nothing.json"),
    include_str!("../designs/light.json"),
    include_str!("../designs/oled.json"),
    include_str!("../designs/ocean.json"),
    include_str!("../designs/forest.json"),
    include_str!("../designs/sepia.json"),
    include_str!("../designs/christmas.json"),
    include_str!("../designs/blueprint.json"),
    include_str!("../designs/chalk.json"),
    include_str!("../designs/night.json"),
    include_str!("../designs/sakura.json"),
    include_str!("../designs/autumn.json"),
    include_str!("../designs/terminal.json"),
    include_str!("../designs/contrast.json"),
];

/// How many of [`builtins`] are the classic six (the pickers put the others under "Besonders").
pub const CLASSICS: usize = 6;

static BUILTINS: OnceLock<Vec<Design>> = OnceLock::new();

/// The built-in designs (read once).
pub fn builtins() -> &'static [Design] {
    BUILTINS.get_or_init(|| {
        let mut out: Vec<Design> = Vec::with_capacity(BUILTIN_FILES.len());
        for file in BUILTIN_FILES {
            // Parsed without the "my-" rename (that check needs this very list).
            let value = extract(file).expect("built-in design is JSON");
            let id = value.get("id").and_then(Value::as_str).expect("built-in design has an id").to_string();
            let mut d = parse_unchecked(file).expect("built-in design parses");
            d.id = id;
            d.builtin = true;
            out.push(d);
        }
        out
    })
}

/// [`parse`] for the built-in files while [`BUILTINS`] is being filled (no id rename).
fn parse_unchecked(text: &str) -> Result<Design, Vec<Problem>> {
    // `parse` looks up builtins() for the id check, which would recurse; the built-in files
    // carry ids that are not in conflict with anything, so read the id afterwards instead.
    PARSING_BUILTINS.with(|f| f.set(true));
    let r = parse(text).map(|p| p.design);
    PARSING_BUILTINS.with(|f| f.set(false));
    r
}

thread_local! {
    static PARSING_BUILTINS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn builtin_ids() -> Vec<&'static str> {
    if PARSING_BUILTINS.with(|f| f.get()) {
        return Vec::new();
    }
    builtins().iter().map(|d| d.id.as_str()).collect()
}

/// The design with this id among the built-ins and `user`'s; Nothing if there is none.
pub fn find<'a>(id: &str, user: &'a [Design]) -> &'a Design {
    builtins().iter().chain(user.iter()).find(|d| d.id == id).unwrap_or(&builtins()[0])
}

// --- imported designs on disk ------------------------------------------------------------------

/// The imported designs in `dir` (`<id>.json` each), by name. Files that no longer read
/// (edited by hand, from a newer n0te) are skipped.
pub fn load_dir(dir: &Path) -> Vec<Design> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Design> = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        if meta.len() as usize > MAX_BYTES {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(parsed) = parse(&text) else { continue };
        let mut d = parsed.design;
        // The file name is the id it was saved under (a later built-in may have taken it).
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()).filter(|s| is_slug(s) && !builtin_ids().contains(s)) {
            d.id = stem.to_string();
        }
        if out.iter().any(|o| o.id == d.id) {
            continue;
        }
        out.push(d);
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    out
}

/// Stores an imported design as `<dir>/<id>.json` (replacing one with the same id), written
/// to a temporary file first so a crash never leaves half a file.
pub fn save(dir: &Path, d: &Design) -> std::io::Result<PathBuf> {
    if !is_slug(&d.id) || d.builtin {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not an importable design id"));
    }
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.json", d.id));
    let tmp = dir.join(format!(".{}.json.tmp", d.id));
    std::fs::write(&tmp, to_json(d))?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Deletes an imported design. Ids are checked, so a crafted id can't reach outside `dir`.
pub fn remove(dir: &Path, id: &str) -> std::io::Result<()> {
    if !is_slug(id) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a design id"));
    }
    match std::fs::remove_file(dir.join(format!("{id}.json"))) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

/// An id for `d` that isn't taken by a built-in or another of `user` (except the one it
/// replaces when the names match): "mein-design", "mein-design-2", …
pub fn free_id(d: &Design, user: &[Design]) -> String {
    let taken = |id: &str| builtin_ids().contains(&id) || user.iter().any(|u| u.id == id && u.name != d.name);
    if !taken(&d.id) {
        return d.id.clone();
    }
    (2..).map(|n| format!("{}-{n}", d.id)).find(|id| !taken(id)).expect("some id is free")
}

// --- writing your own with an AI ---------------------------------------------------------------

/// Instructions to paste into an AI chat (ChatGPT, Claude, Gemini, …) together with a wish
/// ("cosy, like a library at night"). Its answer imports as it is.
pub fn ai_prompt(english: bool) -> String {
    let example = to_json(&builtins()[6]);
    let decor: Vec<&str> = DecorKind::ALL.iter().map(|k| k.id()).collect();
    let decor = decor.join(", ");
    if english {
        format!(
            r##"You design a look ("style") for n0te, a handwriting notes app for tablets and laptops. Describe the style as ONE JSON object in exactly this format and answer with nothing but that JSON (no explanation).

My wish: [DESCRIBE YOUR STYLE HERE, e.g. "calm like a Japanese tea house" or "my school colours, dark blue and gold"]

Format (version 1):
- "n0te_style": 1
- "name": short German name (at most {MAX_NAME} characters); "name_en": English name
- "description" / "description_en": one short line on what it looks like (at most {MAX_DESC} characters)
- "author": optional
- "dark": true for a dark style, false for a light one
- "colors": hex colours "#RRGGBB"
  - "background": the window behind everything
  - "surface": cards, bars, menus and dialogs (close to the background, a bit lighter on dark styles)
  - "surface_2": a step above surface (pressed buttons, fields)
  - "border": thin lines and outlines
  - "text": main text; must have a contrast of at least 4.5:1 with background AND with surface
  - "muted": secondary text (hints, counts), clearly readable but calmer
  - "accent": the one signal colour (main button, selection, sliders)
  - "on_accent": text on the accent (contrast at least 3:1 with accent)
  - "paper": the page colour on screen; MUST be light (white, cream, a very light tint) because ink is dark
- "corners": radii 0 … {MAX_RADIUS}: {{ "buttons": n, "cards": n, "panels": n }} (0 = square, 20+ = pill-shaped buttons)
- "labels": button label voice: "mono_caps" (technical capitals), "sans" (modern), "serif" (bookish)
- "headings": title face: "dot" (dot matrix), "mono", "sans", "serif"
- "fill": how secondary buttons and cards look: "outline" (thin frames), "tonal" (tinted surfaces, no frames), "rule" (hairlines, technical)
- "shadows": true for soft shadows under cards and dialogs
- "decorations": optional, up to {MAX_DECOR}, drawn behind the notebook library: [{{ "kind": "...", "colors": ["#RRGGBB", ...], "amount": 0 … 1 }}]. Kinds: {decor}.

Make it beautiful and consistent: pick a clear mood, a harmonious palette, readable contrast, and corners, labels, headings and fill that match the mood. Example (the built-in Christmas style):

{example}"##
        )
    } else {
        format!(
            r##"Du gestaltest ein Aussehen („Design“) für n0te, eine App für handschriftliche Notizen auf Tablets und Laptops. Beschreibe das Design als EIN JSON-Objekt in genau diesem Format und antworte nur mit diesem JSON (ohne Erklärung).

Mein Wunsch: [BESCHREIBE HIER DEIN DESIGN, z. B. „ruhig wie ein japanisches Teehaus“ oder „meine Schulfarben, Dunkelblau und Gold“]

Format (Version 1):
- "n0te_style": 1
- "name": kurzer deutscher Name (höchstens {MAX_NAME} Zeichen); "name_en": englischer Name
- "description" / "description_en": eine kurze Zeile, wie es aussieht (höchstens {MAX_DESC} Zeichen)
- "author": optional
- "dark": true für ein dunkles Design, false für ein helles
- "colors": Hex-Farben "#RRGGBB"
  - "background": das Fenster hinter allem
  - "surface": Karten, Leisten, Menüs und Dialoge (nah am Hintergrund, bei dunklen Designs etwas heller)
  - "surface_2": eine Stufe über surface (gedrückte Knöpfe, Eingabefelder)
  - "border": dünne Linien und Rahmen
  - "text": Haupttext; Kontrast mindestens 4,5:1 zu background UND zu surface
  - "muted": Nebentext (Hinweise, Anzahlen), gut lesbar, aber ruhiger
  - "accent": die eine Signalfarbe (Hauptknopf, Auswahl, Regler)
  - "on_accent": Text auf dem Akzent (Kontrast mindestens 3:1 zum Akzent)
  - "paper": die Seitenfarbe am Bildschirm; MUSS hell sein (Weiß, Creme, ein ganz heller Ton), denn Tinte ist dunkel
- "corners": Radien 0 … {MAX_RADIUS}: {{ "buttons": n, "cards": n, "panels": n }} (0 = eckig, ab 20 = Pillen-Knöpfe)
- "labels": Stimme der Knopfbeschriftung: "mono_caps" (technische Großbuchstaben), "sans" (modern), "serif" (buchig)
- "headings": Schrift der Titel: "dot" (Punktmatrix), "mono", "sans", "serif"
- "fill": wie Nebenknöpfe und Karten aussehen: "outline" (feine Rahmen), "tonal" (getönte Flächen ohne Rahmen), "rule" (Haarlinien, technisch)
- "shadows": true für weiche Schatten unter Karten und Dialogen
- "decorations": optional, höchstens {MAX_DECOR}, hinter der Notizbuch-Bibliothek: [{{ "kind": "...", "colors": ["#RRGGBB", ...], "amount": 0 … 1 }}]. Arten: {decor}.

Mach es schön und stimmig: eine klare Stimmung, eine harmonische Palette, gut lesbare Kontraste, und Ecken, Beschriftung, Titel und Füllung, die zur Stimmung passen. Beispiel (das eingebaute Weihnachts-Design):

{example}"##
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> Parsed {
        parse(text).unwrap_or_else(|e| panic!("should parse: {:?}", e.iter().map(|p| p.text(true)).collect::<Vec<_>>()))
    }

    fn errs(text: &str) -> Vec<String> {
        match parse(text) {
            Ok(p) => panic!("should fail, got {:?}", p.design.id),
            Err(e) => e.iter().map(|p| p.text(true)).collect(),
        }
    }

    const MINIMAL: &str = r##"{"n0te_style":1,"name":"Mint","colors":{"background":"#0b1512","surface":"#11201b","text":"#e8f5ef","accent":"#2dd4a0"}}"##;

    #[test]
    fn every_builtin_reads_cleanly_and_is_legible() {
        let all = builtins();
        assert_eq!(all.len(), BUILTIN_FILES.len());
        let mut ids: Vec<&str> = all.iter().map(|d| d.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "built-in ids are unique");
        for (file, d) in BUILTIN_FILES.iter().zip(all) {
            PARSING_BUILTINS.with(|f| f.set(true));
            let p = parse(file).unwrap();
            PARSING_BUILTINS.with(|f| f.set(false));
            assert!(p.warnings.is_empty(), "{}: {:?}", d.id, p.warnings);
            assert!(is_slug(&d.id) && d.builtin);
            assert!(!d.name_en.is_empty() && !d.desc.is_empty() && !d.desc_en.is_empty(), "{}: names and descriptions in both languages", d.id);
            let c = d.colors;
            assert!(contrast(c.text, c.background) >= 4.5, "{}: text on background", d.id);
            assert!(contrast(c.text, c.surface) >= 4.5, "{}: text on surface", d.id);
            assert!(contrast(c.on_accent, c.accent) >= 3.0, "{}: on_accent", d.id);
            assert!(luminance(c.paper) >= 0.7, "{}: paper", d.id);
            assert_eq!(d.dark, luminance(c.background) < 0.35, "{}: dark matches the background", d.id);
        }
        assert_eq!(all[0].id, "nothing", "the default design comes first");
        assert!(all.iter().any(|d| d.id == "christmas" && d.decor.iter().any(|x| x.kind == DecorKind::Snow)));
    }

    #[test]
    fn the_classics_keep_their_look() {
        let n = find("nothing", &[]);
        assert_eq!((n.colors.background, n.colors.accent, n.label, n.heading, n.fill), (0x000000, 0xD71921, Label::MonoCaps, Heading::Dot, Fill::Outline));
        let s = find("sepia", &[]);
        assert_eq!((s.colors.paper, s.label, s.heading), (0xFBF6EA, Label::Serif, Heading::Serif));
        assert_eq!(find("oled", &[]).radius, [0.0; 3]);
        assert_eq!(find("no-such-design", &[]).id, "nothing");
    }

    #[test]
    fn written_designs_read_back_the_same() {
        for d in builtins().iter() {
            let p = ok(&to_json(d));
            let mut back = p.design;
            back.id = d.id.clone();
            back.builtin = true;
            assert_eq!(&back, d);
            assert!(p.warnings.is_empty(), "{}: {:?}", d.id, p.warnings);
        }
    }

    #[test]
    fn a_minimal_design_gets_the_rest_derived() {
        let p = ok(MINIMAL);
        let d = p.design;
        assert_eq!(d.id, "mint");
        assert!(d.dark);
        assert_eq!(d.colors.paper, 0xFFFFFF);
        assert_eq!(d.colors.on_accent, 0x111111, "dark text on a light accent");
        assert!(contrast(d.colors.muted, d.colors.background) > 2.0);
        assert_ne!(d.colors.surface_2, d.colors.surface);
        assert_eq!((d.label, d.heading, d.fill), (Label::Sans, Heading::Sans, Fill::Tonal));
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    }

    #[test]
    fn an_ai_answer_pasted_whole_still_reads() {
        let answer = format!(
            "Gern! Hier ist dein Design:\n\n```json\n{}\n```\n\nViel Spaß damit!",
            r##"{
  // a calm one
  "n0te_style": 1,
  "name": "Teehaus",
  "colors": {
    "background": "#f4efe6", /* rice paper */
    "surface": "#fffaf2",
    "text": "#2b2620",
    "accent": "#5b7f4a",
  },
  "decorations": ["leaves",],
}"##
        );
        let d = ok(&answer).design;
        assert_eq!((d.name.as_str(), d.dark, d.decor.len()), ("Teehaus", false, 1));
        let smart = MINIMAL.replace("\"name\":\"Mint\"", "\u{201C}name\u{201D}:\u{201C}Mint\u{201D}");
        assert_eq!(ok(&smart).design.name, "Mint");
        // A URL in a string is not a comment.
        let url = MINIMAL.replace("\"name\":\"Mint\"", "\"name\":\"Mint\",\"author\":\"https://example.org/a\"");
        assert_eq!(ok(&url).design.author, "https://example.org/a");
    }

    #[test]
    fn broken_files_say_everything_that_is_wrong() {
        assert!(errs("")[0].contains("no valid JSON"));
        assert!(errs("[1,2]")[0].contains("no valid JSON"));
        assert!(errs("{\"name\": }")[0].contains("no valid JSON"));
        let e = errs(r##"{"n0te_style":1,"colors":{"background":"#000","surface":"#111","text":"red","accent":"#f00"},"labels":"comic","fill":"glass"}"##);
        assert!(e.iter().any(|m| m.starts_with("name: missing")), "{e:?}");
        assert!(e.iter().any(|m| m.starts_with("colors.text:") && m.contains("not a colour")), "{e:?}");
        assert!(e.iter().any(|m| m.starts_with("labels:") && m.contains("mono_caps")), "{e:?}");
        assert!(e.iter().any(|m| m.starts_with("fill:") && m.contains("outline")), "{e:?}");
        let e = errs(r##"{"name":"X","colors":{"background":"#000"}}"##);
        assert!(["colors.surface: missing", "colors.text: missing", "colors.accent: missing"].iter().all(|m| e.iter().any(|x| x == m)), "{e:?}");
        assert!(errs(r#"{"name":"X"}"#)[0].starts_with("colors: missing"));
        assert!(errs(&MINIMAL.replace("\"n0te_style\":1", "\"n0te_style\":7"))[0].contains("newer"));
        assert!(errs(&" ".repeat(MAX_BYTES + 1))[0].contains("too big"));
    }

    #[test]
    fn unreadable_or_dark_paper_designs_are_refused() {
        let grey = MINIMAL.replace("#e8f5ef", "#1a2a24");
        assert!(errs(&grey).iter().any(|m| m.starts_with("colors.text:") && m.contains("hard to read")));
        let night_paper = MINIMAL.replace("\"accent\"", "\"paper\":\"#202020\",\"accent\"");
        assert!(errs(&night_paper).iter().any(|m| m.starts_with("colors.paper:")));
        let meh = MINIMAL.replace("#e8f5ef", "#55706a");
        let p = ok(&meh);
        assert!(p.warnings.iter().any(|w| w.field == "colors.text"), "3:1 … 4.5:1 only warns");
    }

    #[test]
    fn small_mistakes_are_fixed_up_with_a_warning() {
        let text = MINIMAL.replace(
            "\"colors\":{",
            r##""corners":{"buttons":99,"cards":-3},"labels":"Mono-Caps","decorations":[{"kind":"confetti"},{"kind":"snow","amount":4,"colors":["#fff","nope"]},"snow","stars","grid"],"colors":{"acent":"#fff","##,
        );
        let p = ok(&text);
        let w: Vec<String> = p.warnings.iter().map(|w| w.text(true)).collect();
        assert!(w.iter().any(|m| m.starts_with("colors.acent:") && m.contains("\"accent\"")), "{w:?}");
        assert_eq!(p.design.radius, [32.0, 0.0, 20.0]);
        assert_eq!(p.design.label, Label::MonoCaps);
        assert!(w.iter().any(|m| m.contains("confetti")), "{w:?}");
        let kinds: Vec<DecorKind> = p.design.decor.iter().map(|d| d.kind).collect();
        assert_eq!(kinds, [DecorKind::Snow, DecorKind::Stars, DecorKind::Grid]);
        assert_eq!(p.design.decor[0].amount, 1.0);
        assert_eq!(p.design.decor[0].colors, [0xFFFFFF]);
        assert!(w.iter().any(|m| m.contains("twice")), "{w:?}");
        assert!(ok(&MINIMAL.replace("#2dd4a0", "#2dd4a080")).warnings.iter().any(|w| w.field == "colors.accent"));
    }

    #[test]
    fn colours_read_in_the_usual_spellings() {
        assert_eq!(parse_color("#fff"), Some(0xFFFFFF));
        assert_eq!(parse_color("1E5BD8"), Some(0x1E5BD8));
        assert_eq!(parse_color(" #1e5bd8ff "), Some(0x1E5BD8));
        assert_eq!(parse_color("rgb(30, 91, 216)"), Some(0x1E5BD8));
        for bad in ["", "#", "#12", "#12345", "blue", "rgb(1,2)", "rgb(1,2,300)", "#ggg", "#ääää"] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
        assert_eq!(hex(0x1E5BD8), "#1e5bd8");
        assert!((contrast(0xFFFFFF, 0x000000) - 21.0).abs() < 0.01);
        assert_eq!(mix(0x000000, 0xFFFFFF, 0.5), 0x808080);
    }

    #[test]
    fn ids_are_safe_and_never_take_a_builtin() {
        assert_eq!(slug("Mein Design: Über-Grün!"), "mein-design-ueber-gruen");
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(slug("✨✨"), "");
        let named = |id: &str| ok(&MINIMAL.replace("\"name\":\"Mint\"", &format!("\"name\":\"Mint\",\"id\":{id:?}"))).design.id;
        assert_eq!(named("christmas"), "my-christmas");
        assert_eq!(named("../evil"), "evil");
        assert_eq!(named("✨"), "mint");
        assert_eq!(ok(&MINIMAL.replace("Mint", "✨")).design.id, "design");
        let mut d = ok(MINIMAL).design;
        let other = Design { name: "Other".into(), ..d.clone() };
        assert_eq!(free_id(&d, &[other.clone()]), "mint-2");
        assert_eq!(free_id(&d, &[d.clone()]), "mint", "re-importing the same design replaces it");
        d.id = "nothing".into();
        assert_eq!(free_id(&d, &[]), "nothing-2");
    }

    #[test]
    fn imported_designs_are_stored_listed_and_deleted() {
        let dir = std::env::temp_dir().join(format!("n0te-designs-{}", crate::ids::new_id(3)));
        assert!(load_dir(&dir).is_empty(), "no folder yet");
        let mint = ok(MINIMAL).design;
        let tea = ok(&MINIMAL.replace("Mint", "Apfel")).design;
        save(&dir, &mint).unwrap();
        save(&dir, &tea).unwrap();
        std::fs::write(dir.join("broken.json"), "{ nope").unwrap();
        std::fs::write(dir.join("notes.txt"), MINIMAL).unwrap();
        let listed = load_dir(&dir);
        assert_eq!(listed.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["apfel", "mint"]);
        assert!(listed.iter().all(|d| !d.builtin));
        assert!(save(&dir, &builtins()[0]).is_err(), "built-ins aren't stored");
        assert!(remove(&dir, "../outside").is_err());
        remove(&dir, "mint").unwrap();
        remove(&dir, "mint").unwrap();
        assert_eq!(load_dir(&dir).len(), 1);
        assert!(!dir.join(".mint.json.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_ai_prompt_explains_the_format_and_its_example_imports() {
        for english in [false, true] {
            let p = ai_prompt(english);
            for key in ["\"n0te_style\"", "\"background\"", "\"on_accent\"", "\"paper\"", "\"corners\"", "\"labels\"", "\"headings\"", "\"fill\"", "\"decorations\"", "snow", "scanlines"] {
                assert!(p.contains(key), "{key} in the prompt (english: {english})");
            }
            let d = ok(&p[p.rfind("{\n  \"n0te_style\"").unwrap()..]).design;
            assert_eq!(d.id, "my-christmas");
        }
    }

    /// A tiny deterministic generator, so the robustness tests need no extra crates.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    #[test]
    fn mangled_files_never_panic() {
        let mut rng = Rng(0x9E3779B97F4A7C15);
        let alphabet: Vec<char> = "{}[]\":,#/*\\ \n0123456789abcdefnulltrue-.eE\u{201C}äöü✨".chars().collect();
        let seeds: Vec<String> = builtins().iter().map(to_json).chain([MINIMAL.to_string()]).collect();
        for i in 0..3000 {
            let mut s: Vec<char> = seeds[i % seeds.len()].chars().collect();
            for _ in 0..1 + rng.below(6) {
                let at = rng.below(s.len() + 1);
                match rng.below(3) {
                    0 if at < s.len() => {
                        s.remove(at);
                    }
                    1 => s.insert(at, alphabet[rng.below(alphabet.len())]),
                    _ if at < s.len() => s[at] = alphabet[rng.below(alphabet.len())],
                    _ => {}
                }
            }
            let text: String = s.into_iter().collect();
            if let Ok(p) = parse(&text) {
                // Whatever reads must write and read back again.
                assert!(parse(&to_json(&p.design)).is_ok(), "{text}");
            }
        }
        for _ in 0..500 {
            let text: String = (0..rng.below(80)).map(|_| alphabet[rng.below(alphabet.len())]).collect();
            let _ = parse(&text);
        }
        let deep = format!("{}{}", "{\"a\":".repeat(5000), "}".repeat(5000));
        let _ = parse(&deep);
    }
}
