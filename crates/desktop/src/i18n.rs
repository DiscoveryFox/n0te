//! German and English. The German text is the key (the app was written in German): `tr("Stift")`
//! gives "Pen" in English and "Stift" in German. Texts with values use `{}` placeholders filled
//! in order with [`trf`]; the few German words that need different English words in different
//! places go through [`trc`] with a context. The language follows the system on Windows (the
//! user's UI language); on Linux the apps stay German as they always were; `N0TE_LANG=de|en` or
//! the settings override both.
//! A text missing from the English table ([`en`]) shows in German (so nothing is ever blank);
//! a test makes sure every `tr` in the sources has its English.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, Ordering};

mod en;
pub use en::{EN_CONTEXT, EN_TABLE};

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

/// [`tr`] for a German text that needs other English words in some places: `context` names the
/// place ("Übernehmen" is "Apply" for a page size but "Import" for a bundle: `trc("bundle",
/// "Übernehmen")`). German is always `de`; English without a context entry is [`tr`]'s.
pub fn trc(context: &'static str, de: &'static str) -> &'static str {
    trc_in(lang(), context, de)
}

/// [`trc`] in language `l`.
pub fn trc_in(l: Lang, context: &'static str, de: &'static str) -> &'static str {
    match l {
        Lang::De => de,
        Lang::En => EN_CONTEXT.iter().find(|(c, d, _)| *c == context && *d == de).map_or_else(|| tr_in(l, de), |(_, _, en)| *en),
    }
}

/// [`trc`] with `{}` placeholders filled in order.
pub fn trcf(context: &'static str, de: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    fill(trc(context, de), args)
}

/// `x` with `places` decimals and the language's decimal mark: "8,5" in German, "8.5" in English.
pub fn number(x: f32, places: usize) -> String {
    number_in(lang(), x, places)
}

fn number_in(l: Lang, x: f32, places: usize) -> String {
    let s = format!("{x:.places$}");
    if l == Lang::De { s.replace('.', ",") } else { s }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_match_and_keys_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for (de, en) in EN_TABLE {
            assert_eq!(de.matches("{}").count(), en.matches("{}").count(), "{de:?} → {en:?}");
            assert!(seen.insert(*de), "twice: {de:?}");
            assert!(!en.is_empty(), "no English for {de:?}");
        }
        let mut seen = std::collections::HashSet::new();
        for (context, de, en) in EN_CONTEXT {
            assert_eq!(de.matches("{}").count(), en.matches("{}").count(), "{context}: {de:?} → {en:?}");
            assert!(seen.insert((*context, *de)), "twice: {context}: {de:?}");
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
        // With a context: its own English, else the plain one; German stays.
        assert_eq!(trc_in(Lang::En, "bundle", "Übernehmen"), "Import");
        assert_eq!(tr_in(Lang::En, "Übernehmen"), "Apply");
        assert_eq!(trc_in(Lang::En, "nowhere", "Stift"), "Pen");
        assert_eq!(trc_in(Lang::De, "bundle", "Übernehmen"), "Übernehmen");
        assert_eq!((number_in(Lang::De, 8.47, 1), number_in(Lang::En, 8.47, 1)), ("8,5".to_string(), "8.5".to_string()));
    }

    /// The string literal starting at `s` (just after its opening quote), unescaped, and the rest
    /// after its closing quote.
    fn literal(s: &str) -> Option<(String, &str)> {
        let mut out = String::new();
        let mut chars = s.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => return Some((out, &s[i + 1..])),
                '\\' => match chars.next()?.1 {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    '0' => out.push('\0'),
                    c @ ('\\' | '"' | '\'') => out.push(c),
                    other => panic!("escape \\{other} in a translated text: write it plainly"),
                },
                c => out.push(c),
            }
        }
        None
    }

    /// Every `tr("…")`, `trf("…", …)`, `trc("context", "…")` and `trcf(…)` with a literal in
    /// the crate's sources: (file, context, German).
    fn translated_literals() -> Vec<(String, Option<String>, String)> {
        fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).expect("src").flatten() {
                let p = e.path();
                if p.is_dir() {
                    rust_files(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        files.sort();
        let mut found = Vec::new();
        for path in files {
            let name = path.strip_prefix(&root).unwrap_or(&path).display().to_string();
            // Comments may name `tr("…")` without meaning a text.
            let code: String = std::fs::read_to_string(&path).expect("source").lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
            let mut rest = code.as_str();
            while let Some(at) = rest.find("tr") {
                let before = rest[..at].chars().next_back();
                let after = &rest[at + 2..];
                rest = after;
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let (with_context, after) = match after {
                    a if a.starts_with("cf(") => (true, &a[3..]),
                    a if a.starts_with("c(") => (true, &a[2..]),
                    a if a.starts_with("f(") => (false, &a[2..]),
                    a if a.starts_with('(') => (false, &a[1..]),
                    _ => continue,
                };
                let Some(a) = after.trim_start().strip_prefix('"') else { continue };
                let Some((first, a)) = literal(a) else { continue };
                if !with_context {
                    found.push((name.clone(), None, first));
                    continue;
                }
                let Some(a) = a.trim_start().strip_prefix(',').map(str::trim_start).and_then(|a| a.strip_prefix('"')) else { continue };
                if let Some((de, _)) = literal(a) {
                    found.push((name.clone(), Some(first), de));
                }
            }
        }
        found
    }

    #[test]
    fn every_text_in_the_sources_has_its_english() {
        let found = translated_literals();
        assert!(found.iter().any(|(file, _, de)| file.ends_with("toolbar.rs") && de == "Stift"), "the scan finds the texts");
        let missing: Vec<String> = found
            .iter()
            .filter(|(_, context, de)| match context {
                None => !EN.contains_key(de.as_str()),
                Some(c) => !EN_CONTEXT.iter().any(|(x, d, _)| x == c && d == de),
            })
            .map(|(file, context, de)| format!("{file}: {}{de:?}", context.as_ref().map(|c| format!("[{c}] ")).unwrap_or_default()))
            .collect();
        assert!(missing.is_empty(), "no English in i18n/en.rs for:\n{}", missing.join("\n"));
    }

    #[test]
    fn texts_from_tables_have_their_english() {
        // Shown through `tr(name)` rather than a literal, so the scan above can't see them.
        let mut texts: Vec<&str> = Vec::new();
        texts.extend(crate::theme::PALETTES.iter().flat_map(|t| [t.name, t.style.desc]));
        texts.extend(n0te_core::format::FORMATS.iter().map(|f| f.name));
        texts.extend(n0te_core::pens::PALETTE_NAMES);
        texts.extend(crate::plots::QUICK.iter().map(|q| q.2));
        texts.push(crate::plots::HELP);
        texts.extend(crate::plots::COMMANDS.iter().map(|c| c.3));
        let missing: Vec<&&str> = texts.iter().filter(|t| !EN.contains_key(**t)).collect();
        assert!(missing.is_empty(), "no English in i18n/en.rs for {missing:?}");
    }
}
