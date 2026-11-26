//! The designs (the same as on the tablet, from `n0te_core::design`), each a palette *and* a
//! style — corner radii, how buttons are filled (outlined, tonal, hairline), the voice of labels
//! (mono caps, sans, serif) and the heading face (dot matrix, mono, sans, serif) — plus
//! decorations (decor.rs). People import their own. Every colour, font and control style the UI
//! uses comes from here.

use iced::widget::{button, container, scrollable, slider, text, text_editor, text_input, toggler};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector, color, font};

// --- designs ---------------------------------------------------------------------------------

pub use n0te_core::design::{Fill, Heading, Label};
use n0te_core::design::{Decor, Design};

/// A design's shapes and voice.
#[derive(Debug, Clone, Copy)]
pub struct Style {
    /// Corner radii of buttons, cards and dialogs.
    pub button: f32,
    pub card: f32,
    pub panel: f32,
    pub label: Label,
    pub heading: Heading,
    pub fill: Fill,
    /// Soft shadows under cards and dialogs.
    pub lift: bool,
}

/// A design for the chrome (and the paper's tint on screen; exports stay white): colours,
/// style and decorations. The built-in ones are the core's (`crates/core/designs`, the same as
/// the tablet's); imported ones are read from [`designs_dir`].
#[derive(Debug, Clone)]
pub struct Palette {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub desc: String,
    pub desc_en: String,
    pub author: String,
    pub chrome: Color,
    pub surface: Color,
    pub surface_2: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    /// Text and icons on the accent.
    pub on_accent: Color,
    pub paper: Color,
    pub dark: bool,
    pub style: Style,
    pub decor: Vec<Decor>,
    /// Ships with n0te (imported ones can be copied out and deleted).
    pub builtin: bool,
    /// The design as a file.
    pub json: String,
}

impl Palette {
    /// The name in the app's language.
    pub fn title(&self) -> &str {
        if crate::i18n::lang() == crate::i18n::Lang::En && !self.name_en.is_empty() { &self.name_en } else { &self.name }
    }

    /// What it looks like, in the app's language.
    pub fn about(&self) -> &str {
        if crate::i18n::lang() == crate::i18n::Lang::En && !self.desc_en.is_empty() { &self.desc_en } else { &self.desc }
    }
}

fn rgb(v: u32) -> Color {
    Color::from_rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A design as a palette that lives as long as the program (designs are few and small; one
/// re-imported under the same id simply leaks its old copy).
fn leak(d: &Design) -> &'static Palette {
    let c = d.colors;
    Box::leak(Box::new(Palette {
        id: d.id.clone(),
        name: d.name.clone(),
        name_en: d.name_en.clone(),
        desc: d.desc.clone(),
        desc_en: d.desc_en.clone(),
        author: d.author.clone(),
        chrome: rgb(c.background),
        surface: rgb(c.surface),
        surface_2: rgb(c.surface_2),
        border: rgb(c.border),
        text: rgb(c.text),
        muted: rgb(c.muted),
        accent: rgb(c.accent),
        on_accent: rgb(c.on_accent),
        paper: rgb(c.paper),
        dark: d.dark,
        style: Style { button: d.radius[0], card: d.radius[1], panel: d.radius[2], label: d.label, heading: d.heading, fill: d.fill, lift: d.shadows },
        decor: d.decor.clone(),
        builtin: d.builtin,
        json: n0te_core::design::to_json(d),
    }))
}

static BUILTIN: std::sync::LazyLock<Vec<&'static Palette>> = std::sync::LazyLock::new(|| n0te_core::design::builtins().iter().map(leak).collect());
static IMPORTED: std::sync::RwLock<Vec<&'static Palette>> = std::sync::RwLock::new(Vec::new());
/// The current design: always a leaked [`Palette`] (or null before the first switch).
static CURRENT: std::sync::atomic::AtomicPtr<Palette> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// The designs that ship with n0te, in picker order (the classics first, see [`classics`]).
pub fn builtins() -> &'static [&'static Palette] {
    &BUILTIN
}

/// How many of [`builtins`] are the classics; the rest are the special ones.
pub fn classics() -> usize {
    n0te_core::design::CLASSICS
}

/// The imported designs, by name.
pub fn imported() -> Vec<&'static Palette> {
    IMPORTED.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Every design: built-in, then imported.
pub fn all() -> Vec<&'static Palette> {
    builtins().iter().copied().chain(imported()).collect()
}

/// Where imported designs live: `<config>/designs`.
pub fn designs_dir() -> std::path::PathBuf {
    n0te_platform::paths::config_dir().join("designs")
}

/// Reads the imported designs from `dir` (at start and after an import or delete). The
/// current design stays as it is until the next [`set_theme`].
pub fn load_imported(dir: &std::path::Path) {
    let list: Vec<&'static Palette> = n0te_core::design::load_dir(dir).iter().map(leak).collect();
    *IMPORTED.write().unwrap_or_else(|e| e.into_inner()) = list;
}

/// A palette for a design that isn't stored (yet): the import preview.
pub fn preview(d: &Design) -> &'static Palette {
    leak(d)
}

/// The current theme.
pub fn p() -> &'static Palette {
    let ptr = CURRENT.load(std::sync::atomic::Ordering::Acquire);
    if ptr.is_null() {
        return BUILTIN[0];
    }
    // SAFETY: CURRENT only ever holds pointers made from `&'static Palette`s (leaked boxes that
    // are never freed), see `set_theme`.
    unsafe { &*ptr }
}

/// The current design's shapes and voice.
pub fn s() -> &'static Style {
    &p().style
}

/// Switches the theme (unknown ids: Nothing).
pub fn set_theme(id: &str) {
    let t: &'static Palette = all().into_iter().find(|t| t.id == id).unwrap_or(BUILTIN[0]);
    CURRENT.store(t as *const Palette as *mut Palette, std::sync::atomic::Ordering::Release);
}
pub const INK: Color = color!(0x111114);
pub const PAPER_MUTED: Color = color!(0x8A8A92);
pub const PAPER_RULE: Color = color!(0xE4E4E8);
pub const PAPER_TINT: Color = color!(0xF5F5F7);
/// Page background patterns.
pub const PATTERN: Color = color!(0xD3D6DE);
pub const PATTERN_DOT: Color = color!(0xB4B7C0);

// --- fonts -----------------------------------------------------------------------------------

/// Static instances only: cosmic-text 0.15 (iced 0.14) registers a variable font at its
/// default-instance weight and refuses other weights of that family (it then falls back to a
/// system font), so every weight we use must exist as its own face.
pub const FONT_DATA: [&[u8]; 8] = [
    include_bytes!("../fonts/SpaceGrotesk-Regular.ttf"),
    include_bytes!("../fonts/SpaceGrotesk-Medium.ttf"),
    include_bytes!("../fonts/SpaceGrotesk-Bold.ttf"),
    include_bytes!("../fonts/SpaceMono-Regular.ttf"),
    include_bytes!("../fonts/SpaceMono-Bold.ttf"),
    include_bytes!("../fonts/Doto.ttf"),
    include_bytes!("../fonts/NotoSerif-Regular.ttf"),
    include_bytes!("../fonts/NotoSerif-Medium.ttf"),
];

/// Body text.
pub const SANS: Font = Font::with_name("Space Grotesk");
pub const SANS_MEDIUM: Font = Font { weight: font::Weight::Medium, ..SANS };
pub const SANS_BOLD: Font = Font { weight: font::Weight::Bold, ..SANS };
/// Chrome labels, numbers, code.
pub const MONO: Font = Font::with_name("Space Mono");
pub const MONO_BOLD: Font = Font { weight: font::Weight::Bold, ..MONO };
/// Dot-matrix display face (wordmark). Variable font: only its default weight (Black) matches.
pub const DOT: Font = Font { weight: font::Weight::Black, ..Font::with_name("Doto") };
/// Serif (Sepia's labels, Sepia's and Wald's headings).
pub const SERIF: Font = Font::with_name("Noto Serif");
pub const SERIF_MEDIUM: Font = Font { weight: font::Weight::Medium, ..SERIF };

/// Label sizes (logical px).
pub const LABEL: f32 = 11.0;
pub const SMALL: f32 = 12.0;
pub const BODY: f32 = 16.0;

// --- voice -----------------------------------------------------------------------------------

/// A label in the design's case: mono designs shout ("PDF IMPORTIEREN"), the others don't.
pub fn cased(label: &str) -> String {
    if s().label == Label::MonoCaps { label.to_uppercase() } else { label.to_string() }
}

pub fn label_font() -> Font {
    match s().label {
        Label::MonoCaps => MONO_BOLD,
        Label::Sans => SANS_MEDIUM,
        Label::Serif => SERIF_MEDIUM,
    }
}

pub fn label_size() -> f32 {
    match s().label {
        Label::MonoCaps => LABEL,
        Label::Sans => 13.0,
        Label::Serif => 13.5,
    }
}

/// A button label: write it in sentence case ("PDF importieren"), the design picks the case.
pub fn btn<'a>(label: &str) -> text::Text<'a> {
    text(cased(label)).font(label_font()).size(label_size())
}

/// A compact button label (row actions).
pub fn btn_small<'a>(label: &str) -> text::Text<'a> {
    text(cased(label)).font(label_font()).size(label_size() - 1.5)
}

/// Button padding: mono labels are small and wide, the others want more air.
pub fn pad() -> iced::Padding {
    if s().label == Label::MonoCaps { [6.0, 12.0].into() } else { [7.0, 15.0].into() }
}

/// Small facts (page counts, ages, paths).
pub fn meta_font() -> Font {
    match s().label {
        Label::MonoCaps => MONO,
        Label::Sans => SANS,
        Label::Serif => SERIF,
    }
}

/// Titles of things (notebook cards, options).
pub fn title_font() -> Font {
    if s().heading == Heading::Serif { SERIF_MEDIUM } else { SANS_MEDIUM }
}

/// A small label above a group ("NOTIZBÜCHER" / "Notizbücher").
pub fn caption<'a>(label: &str) -> text::Text<'a> {
    let t = match s().label {
        Label::MonoCaps => text(label.to_uppercase()).font(MONO).size(LABEL),
        Label::Sans => text(label.to_string()).font(SANS_MEDIUM).size(12.5),
        Label::Serif => text(label.to_string()).font(SERIF).size(13.0),
    };
    t.color(p().muted)
}

/// A heading in the design's display face (screen titles, dialog titles).
pub fn heading<'a>(label: &str, size: f32) -> text::Text<'a> {
    let t = match s().heading {
        Heading::Dot => text(label.to_lowercase()).font(DOT).size(size * 1.3),
        Heading::Mono => text(label.to_uppercase()).font(MONO_BOLD).size(size * 0.82),
        Heading::Sans => text(label.to_string()).font(SANS_BOLD).size(size),
        Heading::Serif => text(label.to_string()).font(SERIF_MEDIUM).size(size * 1.05),
    };
    t.color(p().text)
}

/// The secondary surface of tonal designs: a tint of the accent on dark, a grey on light.
fn tonal() -> Color {
    if p().dark { alpha(p().accent, 0.16) } else { p().surface_2 }
}

fn tonal_hover() -> Color {
    if p().dark { alpha(p().accent, 0.26) } else { p().border }
}

fn lift(strength: f32) -> Shadow {
    if s().lift {
        Shadow { color: Color { a: ((if p().dark { 0.30 } else { 0.08 }) * strength).min(0.8), ..Color::BLACK }, offset: Vector::new(0.0, 2.0 * strength), blur_radius: 10.0 * strength }
    } else {
        Shadow::default()
    }
}

pub fn theme() -> Theme {
    let t = p();
    Theme::custom(t.name.clone(), iced::theme::Palette { background: t.chrome, text: t.text, primary: t.accent, success: color!(0x3DDC84), warning: color!(0xF5B400), danger: t.accent })
}

fn border(c: Color, width: f32, radius: f32) -> Border {
    Border { color: c, width, radius: radius.into() }
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

// --- containers ------------------------------------------------------------------------------

pub fn chrome(_: &Theme) -> container::Style {
    container::Style { background: Some(p().chrome.into()), text_color: Some(p().text), ..Default::default() }
}

pub fn surface(_: &Theme) -> container::Style {
    container::Style { background: Some(p().surface.into()), text_color: Some(p().text), ..Default::default() }
}

/// A 1 px hairline (used as a divider between chrome areas).
pub fn hairline(_: &Theme) -> container::Style {
    container::Style { background: Some(p().border.into()), ..Default::default() }
}

pub fn paper(_: &Theme) -> container::Style {
    container::Style {
        background: Some(p().paper.into()),
        text_color: Some(INK),
        border: border(p().border, 1.0, 2.0),
        // No blur shadow: invisible on black anyway, and tiny-skia rasterises the blur for the
        // whole (very tall) page on every frame.
        ..Default::default()
    }
}

pub fn code_box(_: &Theme) -> container::Style {
    container::Style { background: Some(PAPER_TINT.into()), border: border(PAPER_RULE, 1.0, 3.0), ..Default::default() }
}

pub fn formula_box(_: &Theme) -> container::Style {
    container::Style { background: Some(Color::WHITE.into()), border: border(alpha(p().accent, 0.35), 1.0, 3.0), ..Default::default() }
}

/// A small pill (status chips in the chrome).
pub fn chip(_: &Theme) -> container::Style {
    let line = match s().fill {
        Fill::Tonal => Color::TRANSPARENT,
        Fill::Rule => alpha(p().text, 0.3),
        Fill::Outline => p().border,
    };
    container::Style {
        background: Some((if s().fill == Fill::Tonal { tonal() } else { p().surface }).into()),
        text_color: Some(p().muted),
        border: border(line, if line.a > 0.0 { 1.0 } else { 0.0 }, s().button.min(10.0)),
        ..Default::default()
    }
}

// --- buttons ---------------------------------------------------------------------------------

fn is_hover(status: button::Status) -> bool {
    matches!(status, button::Status::Hovered | button::Status::Pressed)
}

/// A toggle button (top bar tabs, choices): the active one stands out in the design's way.
pub fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = is_hover(status);
        let r = s().button;
        match s().fill {
            Fill::Tonal => button::Style {
                background: Some(Background::Color(if active { p().text } else if hovered { tonal_hover() } else { tonal() })),
                text_color: if active { p().chrome } else { p().text },
                border: border(Color::TRANSPARENT, 0.0, r),
                ..Default::default()
            },
            Fill::Outline | Fill::Rule => button::Style {
                background: Some(Background::Color(if active { p().text } else if hovered { p().surface_2 } else { Color::TRANSPARENT })),
                text_color: if active { p().chrome } else if hovered { p().text } else { p().muted },
                border: border(if active { p().text } else if s().fill == Fill::Rule { alpha(p().text, 0.35) } else { p().border }, 1.0, r),
                ..Default::default()
            },
        }
    }
}

/// A secondary action (next to the primary one).
pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    let hovered = is_hover(status);
    let r = s().button;
    match s().fill {
        Fill::Outline => button::Style {
            background: Some(Background::Color(if hovered { p().surface_2 } else { Color::TRANSPARENT })),
            text_color: p().text,
            border: border(if hovered { p().muted } else { p().border }, 1.0, r),
            ..Default::default()
        },
        Fill::Tonal => button::Style {
            background: Some(Background::Color(if hovered { tonal_hover() } else { tonal() })),
            text_color: p().text,
            border: border(Color::TRANSPARENT, 0.0, r),
            ..Default::default()
        },
        // Teenage-Engineering-like: a hairline box that inverts under the pointer.
        Fill::Rule => button::Style {
            background: Some(Background::Color(if hovered { p().text } else { Color::TRANSPARENT })),
            text_color: if hovered { p().chrome } else { p().text },
            border: border(alpha(p().text, 0.5), 1.0, r),
            ..Default::default()
        },
    }
}

/// A row in the outline sidebar.
pub fn row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: (selected || hovered).then(|| Background::Color(if selected { p().surface_2 } else { p().surface })),
            text_color: if selected { p().text } else { alpha(p().text, 0.82) },
            border: border(Color::TRANSPARENT, 0.0, 4.0),
            ..Default::default()
        }
    }
}

/// A row in the library's sidebar: like an outline row, framed in the accent while a dragged
/// card is over it (letting go drops it there).
pub fn drop_row(selected: bool, over: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |t, status| {
        let mut st = row(selected)(t, status);
        if over {
            st.background = Some(Background::Color(alpha(p().accent, 0.14)));
            st.border = border(p().accent, 1.5, 4.0);
            st.text_color = p().text;
        }
        st
    }
}

/// A card in "Auswählen": the ticked ones carry the accent.
pub fn card_ticked(on: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |t, status| {
        let mut st = card(t, status);
        if on {
            st.border = border(p().accent, 2.0, s().card);
        }
        st
    }
}

/// A right-click menu: a small floating sheet with a soft shadow in every design.
pub fn menu(_: &Theme) -> container::Style {
    let line = match s().fill {
        Fill::Rule => alpha(p().text, 0.4),
        Fill::Tonal if !p().dark => Color::TRANSPARENT,
        _ => p().border,
    };
    container::Style {
        background: Some((if p().dark { p().surface_2 } else { p().surface }).into()),
        text_color: Some(p().text),
        border: border(line, if line.a > 0.0 { 1.0 } else { 0.0 }, s().card.min(12.0)),
        shadow: Shadow { color: Color { a: if p().dark { 0.5 } else { 0.16 }, ..Color::BLACK }, offset: Vector::new(0.0, 4.0), blur_radius: 18.0 },
        ..Default::default()
    }
}

/// A row in a right-click menu: quiet until the pointer is on it; `danger` in the accent.
pub fn menu_row(danger: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = is_hover(status);
        let tint = if danger { alpha(p().accent, 0.14) } else if s().fill == Fill::Tonal { tonal() } else { alpha(p().text, if p().dark { 0.09 } else { 0.06 }) };
        button::Style {
            background: hovered.then_some(Background::Color(tint)),
            text_color: if danger { p().accent } else { p().text },
            border: border(Color::TRANSPARENT, 0.0, s().button.min(8.0)),
            ..Default::default()
        }
    }
}

/// A notebook card in the library.
pub fn card(_: &Theme, status: button::Status) -> button::Style {
    let hovered = is_hover(status);
    let r = s().card;
    match s().fill {
        Fill::Outline => button::Style {
            background: Some(Background::Color(if hovered { p().surface_2 } else { p().surface })),
            text_color: p().text,
            border: border(if hovered { p().muted } else { p().border }, 1.0, r),
            ..Default::default()
        },
        Fill::Tonal => button::Style {
            background: Some(Background::Color(if hovered { p().surface_2 } else { p().surface })),
            text_color: p().text,
            border: border(Color::TRANSPARENT, 0.0, r),
            shadow: lift(if hovered { 1.6 } else { 1.0 }),
            ..Default::default()
        },
        Fill::Rule => button::Style {
            background: Some(Background::Color(if hovered { p().surface_2 } else { Color::TRANSPARENT })),
            text_color: p().text,
            border: border(if hovered { p().text } else { alpha(p().text, 0.22) }, 1.0, r),
            ..Default::default()
        },
    }
}

/// The primary action (the accent); faded while it can't be pressed.
pub fn accent_button(_: &Theme, status: button::Status) -> button::Style {
    let hovered = is_hover(status);
    if status == button::Status::Disabled {
        return button::Style {
            background: Some(Background::Color(alpha(p().accent, 0.35))),
            text_color: alpha(p().on_accent, 0.6),
            border: border(Color::TRANSPARENT, 1.0, s().button),
            ..Default::default()
        };
    }
    button::Style {
        background: Some(Background::Color(if hovered { alpha(p().accent, 0.85) } else { p().accent })),
        text_color: p().on_accent,
        border: border(p().accent, 1.0, s().button),
        shadow: lift(0.6),
        ..Default::default()
    }
}

/// A framed panel on the surface (imports list, export, sharing).
pub fn panel(_: &Theme) -> container::Style {
    let (bg, line) = match s().fill {
        Fill::Outline => (p().chrome, p().border),
        Fill::Tonal => (if p().dark { p().surface_2 } else { p().surface }, Color::TRANSPARENT),
        Fill::Rule => (p().chrome, alpha(p().text, 0.4)),
    };
    container::Style {
        background: Some(bg.into()),
        text_color: Some(p().text),
        border: border(line, if line.a > 0.0 { 1.0 } else { 0.0 }, s().panel),
        shadow: lift(2.4),
        ..Default::default()
    }
}

/// A group inside a panel (a settings section, an option tile).
pub fn group(_: &Theme) -> container::Style {
    match s().fill {
        Fill::Tonal => container::Style { background: Some(tonal().into()), border: border(Color::TRANSPARENT, 0.0, s().card.min(16.0)), ..Default::default() },
        Fill::Outline => container::Style { background: Some(p().surface.into()), border: border(p().border, 1.0, s().card.min(16.0)), ..Default::default() },
        Fill::Rule => container::Style { border: border(alpha(p().text, 0.22), 1.0, 0.0), ..Default::default() },
    }
}

/// A selectable option tile (export scope, design): the chosen one carries the accent.
pub fn option(on: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = is_hover(status);
        let base = match s().fill {
            Fill::Tonal => if hovered { tonal_hover() } else { tonal() },
            _ => if hovered { p().surface_2 } else { p().surface },
        };
        let line = if on { p().accent } else if s().fill == Fill::Tonal { Color::TRANSPARENT } else if s().fill == Fill::Rule { alpha(p().text, 0.3) } else { p().border };
        button::Style {
            background: Some(Background::Color(base)),
            text_color: p().text,
            border: border(line, if on { 2.0 } else if line.a > 0.0 { 1.0 } else { 0.0 }, s().card.min(14.0)),
            ..Default::default()
        }
    }
}

/// A quiet action (close, cancel, small row actions).
pub fn ghost(_: &Theme, status: button::Status) -> button::Style {
    let hovered = is_hover(status);
    button::Style {
        background: hovered.then(|| Background::Color(if s().fill == Fill::Tonal { tonal() } else { p().surface_2 })),
        text_color: if hovered { p().text } else { p().muted },
        border: border(if s().fill == Fill::Outline { p().border } else { Color::TRANSPARENT }, 1.0, s().button),
        ..Default::default()
    }
}

/// Text that opens something ("Alle › Uni"): quiet, plain on hover, no frame.
pub fn link(_: &Theme, status: button::Status) -> button::Style {
    button::Style { text_color: if is_hover(status) { p().text } else { p().muted }, ..Default::default() }
}

/// Text fields in the chrome.
pub fn field(_: &Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    let line = if focused { p().accent } else if s().fill == Fill::Rule { alpha(p().text, 0.35) } else { p().border };
    text_input::Style {
        background: Background::Color(if s().fill == Fill::Tonal { tonal() } else { p().chrome }),
        border: border(if s().fill == Fill::Tonal && !focused { Color::TRANSPARENT } else { line }, 1.0, s().button.min(10.0)),
        icon: p().muted,
        placeholder: p().muted,
        value: p().text,
        selection: alpha(p().accent, 0.3),
    }
}

/// On/off switches: a pill (or a square in OLED) with the accent when on.
pub fn switch(_: &Theme, status: toggler::Status) -> toggler::Style {
    let on = matches!(status, toggler::Status::Active { is_toggled: true } | toggler::Status::Hovered { is_toggled: true } | toggler::Status::Disabled { is_toggled: true });
    let square = s().fill == Fill::Rule;
    let (track, knob) = match (s().label, on) {
        // Nothing / OLED: a light track with the red knob, like the tablet.
        (Label::MonoCaps, true) => (p().text, p().accent),
        (Label::MonoCaps, false) => (p().chrome, p().muted),
        (_, true) => (p().accent, p().on_accent),
        (_, false) => (p().border, if p().dark { p().muted } else { Color::WHITE }),
    };
    toggler::Style {
        background: Background::Color(track),
        background_border_width: 1.0,
        background_border_color: if on { track } else if square { alpha(p().text, 0.5) } else { p().border },
        foreground: Background::Color(knob),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: Some(p().text),
        border_radius: square.then(|| 0.0.into()),
        padding_ratio: 0.18,
    }
}

/// Sliders: the accent rail, a round (or square) knob.
pub fn slider_style(_: &Theme, status: slider::Status) -> slider::Style {
    let active = matches!(status, slider::Status::Hovered | slider::Status::Dragged);
    let shape = if s().fill == Fill::Rule { slider::HandleShape::Rectangle { width: 10, border_radius: 0.0.into() } } else { slider::HandleShape::Circle { radius: if active { 8.0 } else { 7.0 } } };
    slider::Style {
        rail: slider::Rail { backgrounds: (p().accent.into(), p().border.into()), width: 3.0, border: border(Color::TRANSPARENT, 0.0, 2.0) },
        handle: slider::Handle { shape, background: p().accent.into(), border_width: if s().label == Label::MonoCaps { 0.0 } else { 2.0 }, border_color: p().chrome },
    }
}

// --- scrollables -----------------------------------------------------------------------------

pub fn scroll_dark(_: &Theme, status: scrollable::Status) -> scrollable::Style {
    let active = matches!(status, scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. });
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller { background: Background::Color(if active { p().muted } else { p().border }), border: border(Color::TRANSPARENT, 0.0, 2.0) },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(p().surface_2),
            border: border(p().border, 1.0, 8.0),
            shadow: Shadow::default(),
            icon: p().text,
        },
    }
}

// --- text editors on paper -------------------------------------------------------------------

pub fn editor_on_paper(_: &Theme, _status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        placeholder: color!(0xB0B0B8),
        value: INK,
        // Drawn over the glyphs, so it must stay translucent.
        selection: alpha(p().accent, 0.20),
    }
}

/// The text box editor: a thin red frame on the paper.
pub fn editor_on_paper_framed(_: &Theme, _status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(Color { a: 0.92, ..Color::WHITE }),
        border: border(alpha(p().accent, 0.7), 1.0, 4.0),
        placeholder: color!(0xB0B0B8),
        value: INK,
        selection: alpha(p().accent, 0.20),
    }
}

/// Small buttons in the text box toolbar.
pub fn text_tool(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered { p().accent } else { Color::WHITE })),
        text_color: if hovered { p().on_accent } else { p().accent },
        border: border(p().accent, 1.0, 6.0),
        ..Default::default()
    }
}

/// Dims the screen under an overlay (palette, help, settings).
pub fn scrim(_: &Theme) -> container::Style {
    container::Style { background: Some(Color { a: 0.45, ..Color::BLACK }.into()), ..Default::default() }
}
