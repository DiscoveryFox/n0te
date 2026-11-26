//! Designs for the Android app: the built-in ones, the imported ones in a folder, reading a
//! file or a pasted AI answer (with errors and warnings to show), storing it, and the prompt
//! that asks an AI for a design. The format and its rules live in `n0te_core::design`.

use std::path::Path;

use n0te_core::design::{self, Design};

use crate::{N0teError, R};

/// A decoration behind the library.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiDecor {
    /// "snow", "lights", "stars", "grid", "petals", "leaves", "scanlines" (more may come:
    /// skip kinds you don't know).
    pub kind: String,
    /// 0xRRGGBB; empty: the app's default colours for the kind.
    pub colors: Vec<u32>,
    /// 0 = a little … 1 = a lot.
    pub amount: f32,
    /// Moves over time.
    pub animated: bool,
}

/// A design. Colours are 0xRRGGBB.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiDesign {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub desc: String,
    pub desc_en: String,
    pub author: String,
    pub dark: bool,
    pub background: u32,
    pub surface: u32,
    pub surface2: u32,
    pub border: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub on_accent: u32,
    pub paper: u32,
    /// Corner radii in dp.
    pub radius_button: f32,
    pub radius_card: f32,
    pub radius_panel: f32,
    /// "mono_caps", "sans", "serif".
    pub label: String,
    /// "dot", "mono", "sans", "serif".
    pub heading: String,
    /// "outline", "tonal", "rule".
    pub fill: String,
    pub shadows: bool,
    pub decor: Vec<FfiDecor>,
    /// Ships with n0te: can't be deleted.
    pub builtin: bool,
    /// The design as a file, to share it.
    pub json: String,
}

impl From<&Design> for FfiDesign {
    fn from(d: &Design) -> Self {
        let c = d.colors;
        FfiDesign {
            id: d.id.clone(),
            name: d.name.clone(),
            name_en: d.name_en.clone(),
            desc: d.desc.clone(),
            desc_en: d.desc_en.clone(),
            author: d.author.clone(),
            dark: d.dark,
            background: c.background,
            surface: c.surface,
            surface2: c.surface_2,
            border: c.border,
            text: c.text,
            muted: c.muted,
            accent: c.accent,
            on_accent: c.on_accent,
            paper: c.paper,
            radius_button: d.radius[0],
            radius_card: d.radius[1],
            radius_panel: d.radius[2],
            label: d.label.id().into(),
            heading: d.heading.id().into(),
            fill: d.fill.id().into(),
            shadows: d.shadows,
            decor: d.decor.iter().map(|x| FfiDecor { kind: x.kind.id().into(), colors: x.colors.clone(), amount: x.amount, animated: x.kind.animated() }).collect(),
            builtin: d.builtin,
            json: design::to_json(d),
        }
    }
}

/// What reading a design file found: the design (none if there are errors), the errors
/// (all of them, ready to hand back to the AI) and warnings about what was adjusted.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiDesignRead {
    pub design: Option<FfiDesign>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// The designs that ship with n0te, in picker order.
#[uniffi::export]
pub fn design_builtins() -> Vec<FfiDesign> {
    design::builtins().iter().map(FfiDesign::from).collect()
}

/// How many of [`design_builtins`] are the classics (the rest are the special ones).
#[uniffi::export]
pub fn design_classics() -> u32 {
    design::CLASSICS as u32
}

/// The designs imported into `dir`, by name.
#[uniffi::export]
pub fn design_imported(dir: String) -> Vec<FfiDesign> {
    design::load_dir(Path::new(&dir)).iter().map(FfiDesign::from).collect()
}

/// Reads a design from a file's text or a whole pasted AI answer, without storing it.
#[uniffi::export]
pub fn design_read(text: String, english: bool) -> FfiDesignRead {
    match design::parse(&text) {
        Ok(p) => FfiDesignRead { design: Some(FfiDesign::from(&p.design)), errors: Vec::new(), warnings: p.warnings.iter().map(|w| w.text(english)).collect() },
        Err(e) => FfiDesignRead { design: None, errors: e.iter().map(|p| p.text(english)).collect(), warnings: Vec::new() },
    }
}

/// Reads and stores a design in `dir` (a design of the same name is replaced; another one
/// with the same id gets "-2"). Returns it as stored.
#[uniffi::export]
pub fn design_import(dir: String, text: String) -> R<FfiDesign> {
    let dir = Path::new(&dir);
    let mut d = design::parse(&text).map_err(|e| N0teError::Failed { msg: e.iter().map(|p| p.text(false)).collect::<Vec<_>>().join("\n") })?.design;
    d.id = design::free_id(&d, &design::load_dir(dir));
    design::save(dir, &d).map_err(|e| N0teError::Failed { msg: e.to_string() })?;
    Ok(FfiDesign::from(&d))
}

/// Deletes an imported design; false if it couldn't be.
#[uniffi::export]
pub fn design_remove(dir: String, id: String) -> bool {
    design::remove(Path::new(&dir), &id).is_ok()
}

/// The instructions to paste into an AI chat, with the format and an example.
#[uniffi::export]
pub fn design_prompt(english: bool) -> String {
    design::ai_prompt(english)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn designs_cross_the_bridge_whole() {
        let all = design_builtins();
        assert!(all.len() >= 14 && all[0].id == "nothing" && all.iter().all(|d| d.builtin));
        assert_eq!(design_classics(), 6);
        let xmas = all.iter().find(|d| d.id == "christmas").unwrap();
        assert!(xmas.decor.iter().any(|d| d.kind == "snow" && d.animated));
        assert_eq!(design_read(xmas.json.clone(), false).design.unwrap().id, "my-christmas");

        let dir = std::env::temp_dir().join(format!("n0te-ffi-designs-{}", std::process::id()));
        let dir_s = dir.to_string_lossy().to_string();
        let r = design_read("{ \"name\": \"X\" }".into(), true);
        assert!(r.design.is_none() && r.errors.iter().any(|e| e.starts_with("colors")));
        assert!(design_import(dir_s.clone(), "nope".into()).is_err());
        let mine = r##"{"name":"Mint","colors":{"background":"#0b1512","surface":"#11201b","text":"#e8f5ef","accent":"#2dd4a0"}}"##;
        let a = design_import(dir_s.clone(), mine.into()).unwrap();
        let again = design_import(dir_s.clone(), mine.into()).unwrap();
        assert_eq!((a.id.as_str(), again.id.as_str()), ("mint", "mint"), "the same design is replaced");
        let other = design_import(dir_s.clone(), mine.replace("\"Mint\"", "\"Mint\",\"id\":\"mint\",\"author\":\"x\"").replace("Mint", "Minze")).unwrap();
        assert_eq!(other.id, "mint-2");
        assert_eq!(design_imported(dir_s.clone()).len(), 2);
        assert!(design_remove(dir_s.clone(), "mint".into()));
        assert!(!design_remove(dir_s.clone(), "../x".into()));
        assert_eq!(design_imported(dir_s.clone()).len(), 1);
        assert!(design_prompt(false).contains("Mein Wunsch"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
