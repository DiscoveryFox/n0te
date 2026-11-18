//! The "Nothing" look: black chrome, white paper, one red accent, monospace labels and a
//! dot-matrix wordmark. Every colour and font the UI uses comes from here, so a theme file
//! can later replace these tokens without touching the widgets.

use iced::widget::{button, container, scrollable, text_editor};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector, color, font};

// --- colour tokens --------------------------------------------------------------------------

pub const CHROME: Color = color!(0x000000);
pub const SURFACE: Color = color!(0x141416);
pub const SURFACE_2: Color = color!(0x1E1E22);
pub const BORDER: Color = color!(0x262629);
pub const TEXT: Color = color!(0xF2F2F4);
pub const MUTED: Color = color!(0x787880);
pub const ACCENT: Color = color!(0xD71921);

/// Page colours (the paper is always white, also in the dark chrome).
pub const PAPER: Color = color!(0xFFFFFF);
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
    Theme::custom(
        "Nothing",
        iced::theme::Palette { background: CHROME, text: TEXT, primary: ACCENT, success: color!(0x3DDC84), warning: color!(0xF5B400), danger: ACCENT },
    )
}

fn border(c: Color, width: f32, radius: f32) -> Border {
    Border { color: c, width, radius: radius.into() }
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

// --- containers ------------------------------------------------------------------------------

pub fn chrome(_: &Theme) -> container::Style {
    container::Style { background: Some(CHROME.into()), text_color: Some(TEXT), ..Default::default() }
}

pub fn surface(_: &Theme) -> container::Style {
    container::Style { background: Some(SURFACE.into()), text_color: Some(TEXT), ..Default::default() }
}

/// A 1 px hairline (used as a divider between chrome areas).
pub fn hairline(_: &Theme) -> container::Style {
    container::Style { background: Some(BORDER.into()), ..Default::default() }
}

pub fn paper(_: &Theme) -> container::Style {
    container::Style {
        background: Some(PAPER.into()),
        text_color: Some(INK),
        border: border(BORDER, 1.0, 2.0),
        shadow: Shadow { color: alpha(Color::BLACK, 0.6), offset: Vector::new(0.0, 8.0), blur_radius: 32.0 },
        ..Default::default()
    }
}

pub fn code_box(_: &Theme) -> container::Style {
    container::Style { background: Some(PAPER_TINT.into()), border: border(PAPER_RULE, 1.0, 3.0), ..Default::default() }
}

pub fn formula_box(_: &Theme) -> container::Style {
    container::Style { background: Some(Color::WHITE.into()), border: border(alpha(ACCENT, 0.35), 1.0, 3.0), ..Default::default() }
}

/// A small pill (status chips in the chrome).
pub fn chip(_: &Theme) -> container::Style {
    container::Style { background: Some(SURFACE.into()), text_color: Some(MUTED), border: border(BORDER, 1.0, 10.0), ..Default::default() }
}

// --- buttons ---------------------------------------------------------------------------------

/// Segmented tab in the top bar.
pub fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if active { TEXT } else if hovered { SURFACE_2 } else { CHROME })),
            text_color: if active { CHROME } else if hovered { TEXT } else { MUTED },
            border: border(if active { TEXT } else { BORDER }, 1.0, 14.0),
            ..Default::default()
        }
    }
}

/// A row in the outline sidebar.
pub fn row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: (selected || hovered).then(|| Background::Color(if selected { SURFACE_2 } else { SURFACE })),
            text_color: if selected { TEXT } else { color!(0xC8C8CE) },
            border: border(Color::TRANSPARENT, 0.0, 4.0),
            ..Default::default()
        }
    }
}

pub fn ghost(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then(|| Background::Color(SURFACE_2)),
        text_color: if hovered { TEXT } else { MUTED },
        border: border(BORDER, 1.0, 14.0),
        ..Default::default()
    }
}

// --- scrollables -----------------------------------------------------------------------------

pub fn scroll_dark(_: &Theme, status: scrollable::Status) -> scrollable::Style {
    let active = matches!(status, scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. });
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller { background: Background::Color(if active { MUTED } else { BORDER }), border: border(Color::TRANSPARENT, 0.0, 2.0) },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(SURFACE_2),
            border: border(BORDER, 1.0, 8.0),
            shadow: Shadow::default(),
            icon: TEXT,
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
        selection: alpha(ACCENT, 0.20),
    }
}
