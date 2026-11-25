//! Page formats: the paper sizes offered when a notebook or page is made or changed, the same on
//! every device. Sizes are page points (1 pt = 1/72 inch), stored upright (width ≤ height);
//! landscape swaps them.

/// A named paper size, upright.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Format {
    pub id: &'static str,
    pub name: &'static str,
    pub width: f32,
    pub height: f32,
}

/// The smallest and largest side a page may have: 1 inch, and 200 inches (about 5 m: the
/// largest page PDF viewers show reliably, so every page still exports).
pub const MIN_PT: f32 = 72.0;
pub const MAX_PT: f32 = 14400.0;
/// [`MAX_PT`] in millimetres.
pub const MAX_MM: f32 = 5080.0;

pub const FORMATS: &[Format] = &[
    Format { id: "a4", name: "A4", width: 595.0, height: 842.0 },
    Format { id: "a5", name: "A5", width: 420.0, height: 595.0 },
    Format { id: "a3", name: "A3", width: 842.0, height: 1191.0 },
    Format { id: "a6", name: "A6", width: 298.0, height: 420.0 },
    Format { id: "letter", name: "Letter", width: 612.0, height: 792.0 },
    Format { id: "square", name: "Quadrat", width: 595.0, height: 595.0 },
    Format { id: "slide", name: "Folie 16:9", width: 540.0, height: 960.0 },
];

pub fn pt_to_mm(pt: f32) -> f32 {
    pt * 25.4 / 72.0
}

pub fn mm_to_pt(mm: f32) -> f32 {
    mm * 72.0 / 25.4
}

/// The format a page of `width` × `height` pt has (either way round), and whether it lies in
/// landscape. Within 1.5 pt, so formats made from millimetres still match.
pub fn find(width: f32, height: f32) -> Option<(&'static Format, bool)> {
    let near = |a: f32, b: f32| (a - b).abs() <= 1.5;
    FORMATS.iter().find_map(|f| {
        if near(width, f.width) && near(height, f.height) {
            Some((f, false))
        } else if near(width, f.height) && near(height, f.width) {
            Some((f, true))
        } else {
            None
        }
    })
}

/// "A4 hoch", "A5 quer", "Quadrat", "Folie 16:9", or "200 × 150 mm" for a size of its own.
pub fn describe(width: f32, height: f32) -> String {
    match find(width, height) {
        Some((f, _)) if f.width == f.height => f.name.to_string(),
        // A slide is landscape by nature: only say so when it stands upright.
        Some((f, landscape)) if f.id == "slide" => if landscape { f.name.to_string() } else { format!("{} hoch", f.name) },
        Some((f, landscape)) => format!("{} {}", f.name, if landscape { "quer" } else { "hoch" }),
        None => format!("{:.0} × {:.0} mm", pt_to_mm(width), pt_to_mm(height)),
    }
}

/// `width` × `height` turned upright (`landscape` false) or on its side.
pub fn oriented(width: f32, height: f32, landscape: bool) -> (f32, f32) {
    let (short, long) = if width <= height { (width, height) } else { (height, width) };
    if landscape { (long, short) } else { (short, long) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_found_either_way_round_and_described() {
        assert_eq!(find(595.0, 842.0).map(|(f, l)| (f.id, l)), Some(("a4", false)));
        assert_eq!(find(842.0, 595.0).map(|(f, l)| (f.id, l)), Some(("a4", true)));
        // A4 from a PDF (595.28 × 841.89) and from millimetres (210 × 297) is still A4.
        assert_eq!(find(595.28, 841.89).map(|(f, _)| f.id), Some("a4"));
        assert_eq!(find(mm_to_pt(210.0), mm_to_pt(297.0)).map(|(f, _)| f.id), Some("a4"));
        assert_eq!(describe(842.0, 595.0), "A4 quer");
        assert_eq!(describe(420.0, 595.0), "A5 hoch");
        assert_eq!(describe(595.0, 595.0), "Quadrat");
        assert_eq!(describe(960.0, 540.0), "Folie 16:9");
        assert_eq!(describe(540.0, 960.0), "Folie 16:9 hoch");
        assert_eq!(describe(mm_to_pt(200.0), mm_to_pt(150.0)), "200 × 150 mm");
    }

    #[test]
    fn turning_swaps_the_sides_and_every_format_fits_the_limits() {
        assert_eq!(oriented(595.0, 842.0, true), (842.0, 595.0));
        assert_eq!(oriented(842.0, 595.0, false), (595.0, 842.0));
        assert_eq!(oriented(842.0, 595.0, true), (842.0, 595.0));
        for f in FORMATS {
            assert!(f.width <= f.height, "{} is stored upright", f.id);
            assert!(f.width >= MIN_PT && f.height <= MAX_PT);
        }
    }
}
