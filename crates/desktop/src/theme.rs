//! The "Nothing" look: black chrome, white paper, one red accent, monospace labels and a
//! dot-matrix wordmark. Every colour and font the UI uses comes from here, so a theme file
//! can later replace these tokens without touching the widgets.

use iced::widget::{button, container, scrollable, text_editor};
use iced::{Background, Border, Color, Font, Shadow, Theme, color, font};

// --- colour tokens --------------------------------------------------------------------------

/// A colour theme for the chrome (and the paper's tint on screen; exports stay white). The
/// same six as on the tablet.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub id: &'static str,
    pub name: &'static str,
    pub chrome: Color,
    pub surface: Color,
    pub surface_2: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub paper: Color,
    pub dark: bool,
}

const fn pal(id: &'static str, name: &'static str, c: [u32; 8], dark: bool) -> Palette {
    const fn rgb(v: u32) -> Color {
        Color::from_rgb(((v >> 16) & 0xFF) as f32 / 255.0, ((v >> 8) & 0xFF) as f32 / 255.0, (v & 0xFF) as f32 / 255.0)
    }
    Palette { id, name, chrome: rgb(c[0]), surface: rgb(c[1]), surface_2: rgb(c[2]), border: rgb(c[3]), text: rgb(c[4]), muted: rgb(c[5]), accent: rgb(c[6]), paper: rgb(c[7]), dark }
}

pub const PALETTES: [Palette; 6] = [
    pal("nothing", "Nothing", [0x000000, 0x141416, 0x1E1E22, 0x262629, 0xF2F2F4, 0x787880, 0xD71921, 0xFFFFFF], true),
    pal("light", "Hell", [0xF1F1EF, 0xFFFFFF, 0xE8E8E5, 0xD6D6D1, 0x111111, 0x6E6E73, 0xD71921, 0xFFFFFF], false),
    pal("oled", "OLED", [0x000000, 0x000000, 0x101010, 0x1E1E1E, 0xFFFFFF, 0x8A8A8A, 0xFF2D2D, 0xFFFFFF], true),
    pal("ocean", "Ozean", [0x080D17, 0x0F1626, 0x182235, 0x233048, 0xE8EEF8, 0x7C8AA3, 0x3B82F6, 0xFFFFFF], true),
    pal("forest", "Wald", [0x0A100C, 0x121A15, 0x1A241E, 0x25312A, 0xEAF2EC, 0x7F9486, 0x22C55E, 0xFFFFFF], true),
    pal("sepia", "Sepia", [0xEDE3D1, 0xF7F0E3, 0xE6D9C2, 0xD5C4A6, 0x2B2118, 0x7A6A55, 0xB45309, 0xFBF6EA], false),
];

static CURRENT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The current theme.
pub fn p() -> &'static Palette {
    &PALETTES[CURRENT.load(std::sync::atomic::Ordering::Relaxed).min(PALETTES.len() - 1)]
}

/// Switches the theme (unknown ids: Nothing).
pub fn set_theme(id: &str) {
    let i = PALETTES.iter().position(|t| t.id == id).unwrap_or(0);
    CURRENT.store(i, std::sync::atomic::Ordering::Relaxed);
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
pub const FONT_DATA: [&[u8]; 6] = [
    include_bytes!("../fonts/SpaceGrotesk-Regular.ttf"),
    include_bytes!("../fonts/SpaceGrotesk-Medium.ttf"),
    include_bytes!("../fonts/SpaceGrotesk-Bold.ttf"),
    include_bytes!("../fonts/SpaceMono-Regular.ttf"),
    include_bytes!("../fonts/SpaceMono-Bold.ttf"),
    include_bytes!("../fonts/Doto.ttf"),
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

/// Label sizes (logical px).
pub const LABEL: f32 = 11.0;
pub const SMALL: f32 = 12.0;
pub const BODY: f32 = 16.0;

pub fn theme() -> Theme {
    let t = p();
    Theme::custom(t.name, iced::theme::Palette { background: t.chrome, text: t.text, primary: t.accent, success: color!(0x3DDC84), warning: color!(0xF5B400), danger: t.accent })
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
    container::Style { background: Some(p().surface.into()), text_color: Some(p().muted), border: border(p().border, 1.0, 10.0), ..Default::default() }
}

// --- buttons ---------------------------------------------------------------------------------

/// Segmented tab in the top bar.
pub fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if active { p().text } else if hovered { p().surface_2 } else { p().chrome })),
            text_color: if active { p().chrome } else if hovered { p().text } else { p().muted },
            border: border(if active { p().text } else { p().border }, 1.0, 14.0),
            ..Default::default()
        }
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

/// A notebook card in the library.
pub fn card(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered { p().surface_2 } else { p().surface })),
        text_color: p().text,
        border: border(if hovered { p().muted } else { p().border }, 1.0, 16.0),
        ..Default::default()
    }
}

/// The primary action (red).
pub fn accent_button(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered { alpha(p().accent, 0.85) } else { p().accent })),
        text_color: Color::WHITE,
        border: border(p().accent, 1.0, 14.0),
        ..Default::default()
    }
}

/// A framed panel on the surface (imports list).
pub fn panel(_: &Theme) -> container::Style {
    container::Style { background: Some(p().chrome.into()), text_color: Some(p().text), border: border(p().border, 1.0, 14.0), ..Default::default() }
}

pub fn ghost(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then(|| Background::Color(p().surface_2)),
        text_color: if hovered { p().text } else { p().muted },
        border: border(p().border, 1.0, 14.0),
        ..Default::default()
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
        text_color: if hovered { Color::WHITE } else { p().accent },
        border: border(p().accent, 1.0, 6.0),
        ..Default::default()
    }
}

/// Dims the screen under an overlay (palette, help, settings).
pub fn scrim(_: &Theme) -> container::Style {
    container::Style { background: Some(Color { a: 0.45, ..Color::BLACK }.into()), ..Default::default() }
}
