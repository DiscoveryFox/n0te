//! German and English. The German text is the key (the app was written in German): `tr("Stift")`
//! gives "Pen" in English and "Stift" in German. Texts with values use `{}` placeholders filled
//! in order with [`trf`]. The language follows the system on Windows (the user's UI language);
//! on Linux the apps stay German as they always were; `N0TE_LANG=de|en` or the settings
//! override both.
//! A text missing from the English table shows in German (so nothing is ever blank).

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

/// 0 = not decided yet (system), 1 = German, 2 = English.
static LANG: AtomicU8 = AtomicU8::new(0);

/// The language in use.
pub fn lang() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::De,
        2 => Lang::En,
        _ => {
            let l = system();
            set(l);
            l
        }
    }
}

/// Sets the language (settings; "auto" goes back to the system's).
pub fn set(l: Lang) {
    LANG.store(if l == Lang::De { 1 } else { 2 }, Ordering::Relaxed);
}

/// From a setting: "de", "en", or anything else = the system's.
pub fn set_from(setting: &str) {
    match setting {
        "de" => set(Lang::De),
        "en" => set(Lang::En),
        _ => set(system()),
    }
}

/// The system's language: German if it is German, else English.
pub fn system() -> Lang {
    if let Ok(v) = std::env::var("N0TE_LANG") {
        return if v.starts_with("de") { Lang::De } else { Lang::En };
    }
    #[cfg(windows)]
    {
        // SAFETY: plain query without arguments.
        let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
        // Primary language 0x07 = German.
        if id & 0x3ff == 0x07 { Lang::De } else { Lang::En }
    }
    #[cfg(not(windows))]
    {
        Lang::De
    }
}

static EN: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| EN_TABLE.iter().copied().collect());

/// The text in the current language (`de` is the German text and the key).
pub fn tr(de: &'static str) -> &'static str {
    tr_in(lang(), de)
}

/// The text in language `l`.
pub fn tr_in(l: Lang, de: &'static str) -> &'static str {
    match l {
        Lang::De => de,
        Lang::En => EN.get(de).copied().unwrap_or(de),
    }
}

/// [`tr`] with `{}` placeholders filled in order.
pub fn trf(de: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    fill(tr(de), args)
}

fn fill(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut rest = template;
    let mut args = args.iter();
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(a) = args.next() {
            out.push_str(&a.to_string());
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// German → English. Keep it sorted by area; a test checks the placeholders match.
pub static EN_TABLE: &[(&str, &str)] = &[
    // n0te Tablet: tools
    ("Stift", "Pen"),
    ("Marker", "Highlighter"),
    ("Radierer", "Eraser"),
    ("Lasso", "Lasso"),
    ("Text", "Text"),
    ("Laser", "Laser"),
    ("Füller", "Fountain pen"),
    ("Kugelschreiber", "Ballpoint"),
    ("Bleistift", "Pencil"),
    ("Pinsel", "Brush"),
    ("Strich folgt dem Druck", "Line follows the pressure"),
    ("Gleichmäßiger Strich", "Even line"),
    ("Graphit: weich am Rand, dunkler bei Druck", "Graphite: soft edges, darker with pressure"),
    ("Stark druckabhängig", "Strongly pressure-sensitive"),
    ("Ganze Striche", "Whole strokes"),
    ("Fläche", "Area"),
    ("Mit dem Finger schreiben", "Write with a finger"),
    ("{} ausgewählt", "{} selected"),
    ("Kopieren", "Copy"),
    ("Ausschneiden", "Cut"),
    ("Einfügen", "Paste"),
    ("Löschen", "Delete"),
    ("Fertig", "Done"),
    ("{} Striche kopiert", "{} strokes copied"),
    ("Nur lesen: ändern kann, wer dich eingeladen hat", "Read only: only the person who invited you can change it"),
    ("Seite nicht lesbar", "Page can't be read"),
    ("Nicht geändert: {}", "Not changed: {}"),
    ("Rückgängig ging nicht: {}", "Undo failed: {}"),
    ("Linie frei: rastet nicht mehr ein", "Line free: no more snapping"),
    ("Linie rastet wieder ein", "Line snaps again"),
    // Palettes
    ("Standard", "Standard"),
    ("Pastell", "Pastel"),
    ("Neon", "Neon"),
    ("Erde", "Earth"),
    ("Grau", "Grey"),
    ("Textmarker", "Highlighter"),
    // Pairing
    ("Code eines anderen Geräts einfügen (n0te-pair:…)", "Paste another device's code (n0te-pair:…)"),
    ("Koppeln", "Pair"),
    ("Gekoppelt", "Paired"),
    ("Koppeln: {}", "Pairing: {}"),
    ("Gerät koppeln (Code zeigen)", "Pair a device (show code)"),
    ("Tablet koppeln", "Pair a tablet"),
    // Settings
    ("Sprache", "Language"),
    ("Automatisch: unter Windows die Sprache des Systems.", "Automatic: on Windows, the system's language."),
    ("Automatisch", "Automatic"),
    ("Deutsch", "Deutsch"),
    ("English", "English"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_match_and_keys_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for (de, en) in EN_TABLE {
            assert_eq!(de.matches("{}").count(), en.matches("{}").count(), "{de:?} → {en:?}");
            assert!(seen.insert(*de), "twice: {de:?}");
        }
    }

    #[test]
    fn english_and_german_and_fallback() {
        // Without touching the app-wide language (tests run side by side).
        assert_eq!(tr_in(Lang::En, "Stift"), "Pen");
        assert_eq!(fill(tr_in(Lang::En, "{} ausgewählt"), &[&3]), "3 selected");
        assert_eq!(tr_in(Lang::En, "Nicht übersetzt"), "Nicht übersetzt");
        assert_eq!(tr_in(Lang::De, "Stift"), "Stift");
        assert_eq!(fill(tr_in(Lang::De, "{} ausgewählt"), &[&3]), "3 ausgewählt");
    }
}
