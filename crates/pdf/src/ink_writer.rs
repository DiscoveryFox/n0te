//! n0te's own ink writer: appends ink to a PDF (in practice one pdfium just assembled) as an
//! incremental update, without pdfium.
//!
//! pdfium's page-object API regenerates a page's whole content stream and, for pages with an
//! indirect `/Resources` dictionary, walks the entire document on every regeneration, so inking N
//! pages cost O(N × document size). This writer only reads the page tree and the dictionaries it
//! changes, so the cost is linear in the ink and independent of what is already on the pages.
//!
//! # What is written
//! For every page with strokes:
//! - The existing content is wrapped in `q … Q` (two tiny streams shared by all pages, put before
//!   and after the original `/Contents`), so whatever graphics state the original content leaves
//!   behind (an unbalanced `cm`, a clip, a colour) cannot leak into the ink.
//! - One Flate-compressed ink stream is appended:
//!   ```text
//!   /OC /N0teInk BDC          marked content: optional content group "n0te ink"
//!   q
//!   a b c d e f cm            page coordinates -> user space (see below)
//!   r g b rg                  colour, only when it changes
//!   /N0teA128 gs              ExtGState with /ca = alpha, only when alpha changes
//!   x y m  x y l … f          one filled outline per stroke, non-zero winding
//!   …
//!   Q
//!   EMC
//!   ```
//!   Coordinates are n0te page coordinates (points, top-left origin of the displayed page,
//!   y down) rounded to 1/100 pt. The `cm` maps them to the page's user space, so the stroke
//!   geometry itself is the same whatever the page's boxes and rotation are.
//! - The page dictionary is rewritten with the new `/Contents` array and a `/Resources`
//!   dictionary that is a copy of the effective one (own or inherited) plus the ink's
//!   `/Properties` and `/ExtGState` entries (names are made unique if the page already uses
//!   them). Shared resource dictionaries are never modified.
//!
//! # Coordinates
//! The page as displayed is the crop box intersected with the media box (pdfium's rule; missing
//! boxes default to the media box and US Letter), turned clockwise by `/Rotate` (a multiple of
//! 90, inherited through the page tree like the boxes and the resources). With that box
//! `[x0 y0 x1 y1]` the `cm` is:
//!
//! | /Rotate | cm                     | page (X, Y) -> user (x, y) |
//! |---------|------------------------|----------------------------|
//! | 0       | `1 0 0 -1 x0 y1`       | (x0 + X, y1 − Y)           |
//! | 90      | `0 1 1 0 x0 y0`        | (x0 + Y, y0 + X)           |
//! | 180     | `-1 0 0 1 x1 y0`       | (x1 − X, y0 + Y)           |
//! | 270     | `0 -1 -1 0 x1 y1`      | (x1 − Y, y1 − X)           |
//!
//! # Tagging (recognising n0te ink on re-import)
//! All ink of a document belongs to one optional content group (OCG)
//! `<</Type/OCG/Name(n0te ink)/Intent/View/N0te 1>>`, listed in the catalog's `/OCProperties`
//! (`/OCGs`, and `/D /Order` so viewers show a "n0te ink" layer that can be hidden). Each page's
//! ink stream is exactly one marked-content sequence `/OC /<name> BDC … EMC` whose properties
//! resource is that OCG. A re-import recognises n0te ink as: content marked with an OCG whose
//! `/Name` is `n0te ink` (the private `/N0te 1` key distinguishes it from a user layer of the same
//! name). Hiding that OCG (setting it OFF in `/D`, see [`hide_ink`]) renders the page without
//! the ink.
//!
//! # Input requirements
//! A classic cross-reference table (what pdfium writes; cross-reference streams and object
//! streams are refused), not encrypted.

use std::collections::{BTreeMap, HashSet};
use std::io::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use flate2::Compression;
use flate2::write::ZlibEncoder;
use n0te_core::ink;

use crate::export::Stroke;
use crate::syntax::{Dict, File, Obj, write_num, write_uint};
use crate::{Error, Result};

/// `/Name` of the optional content group that holds all n0te ink.
pub const INK_LAYER_NAME: &str = "n0te ink";

/// The ruling of a page drawn under the ink (outside the ink layer, so hiding the ink keeps it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paper {
    pub background: n0te_core::notebook::Background,
    /// Line / box / dot distance in points.
    pub spacing: f32,
}
/// Base name of the ink's `/Properties` resource (made unique per page if taken).
const OC_RESOURCE: &str = "N0teInk";
/// zlib level for ink streams. Measured on 50 slides x 100 strokes: level 1 is 3.5x faster than
/// level 6 (80 vs 280 ms for the whole export) for ~20 % more ink bytes (6.1 vs 5.1 MB).
const FLATE_LEVEL: u32 = 1;

fn bad(msg: impl Into<String>) -> Error {
    Error::Invalid(format!("pdf structure: {}", msg.into()))
}

/// Appends the strokes of `pages[i]` to page `i` of `pdf` (see the module docs) and returns the
/// new file. `pages` must have one entry per page of `pdf`. Without any strokes `pdf` comes back
/// unchanged.
pub fn add_ink(pdf: Vec<u8>, pages: &[&[Stroke<'_>]]) -> Result<Vec<u8>> {
    add_ink_and_paper(pdf, pages, &[])
}

/// [`add_ink`] plus a paper ruling per page (`papers[i]`, missing entries = none) drawn under
/// the ink. Pages with paper but no strokes get the paper.
pub fn add_ink_and_paper(pdf: Vec<u8>, pages: &[&[Stroke<'_>]], papers: &[Option<Paper>]) -> Result<Vec<u8>> {
    add_ink_paper_text(pdf, pages, papers, &[])
}

/// [`add_ink_and_paper`] plus text boxes per page (`texts[i]`), typeset with the standard PDF
/// fonts inside the ink layer, under the strokes.
pub fn add_ink_paper_text(pdf: Vec<u8>, pages: &[&[Stroke<'_>]], papers: &[Option<Paper>], texts: &[&[crate::text_layout::PdfText]]) -> Result<Vec<u8>> {
    add_ink_paper_text_images(pdf, pages, papers, texts, &[])
}

/// An image on an exported page: JPEG bytes (embedded as they are) placed at (x, y), w × h
/// points in displayed-page coordinates. Other formats are skipped.
#[derive(Debug, Clone)]
pub struct PdfImage {
    pub jpeg: Vec<u8>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Width, height and colour components of a baseline or progressive JPEG.
pub fn jpeg_info(data: &[u8]) -> Option<(u32, u32, u8)> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 9 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if matches!(marker, 0xC0 | 0xC1 | 0xC2) {
            let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            return (w > 0 && h > 0).then_some((w, h, data[i + 9]));
        }
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
        } else {
            i += 2 + len;
        }
    }
    None
}

/// [`add_ink_paper_text`] plus images per page (`images[i]`), drawn over the paper and under
/// text and ink (outside the ink layer: they are part of the page).
pub fn add_ink_paper_text_images(mut pdf: Vec<u8>, pages: &[&[Stroke<'_>]], papers: &[Option<Paper>], texts: &[&[crate::text_layout::PdfText]], images: &[&[PdfImage]]) -> Result<Vec<u8>> {
    let has_paper = papers.iter().any(|p| p.is_some_and(|p| p.background != n0te_core::notebook::Background::Blank));
    let has_text = texts.iter().any(|t| !t.is_empty());
    let has_images = images.iter().any(|i| i.iter().any(|im| jpeg_info(&im.jpeg).is_some()));
    if pages.iter().all(|s| s.is_empty()) && !has_paper && !has_text && !has_images {
        return Ok(pdf);
    }
    let update = build_update(&pdf, pages, papers, texts, images)?;
    pdf.extend_from_slice(&update);
    Ok(pdf)
}

/// One leaf of the page tree with the attributes it inherits.
struct PageNode<'a> {
    num: u32,
    generation: u16,
    dict: Dict<'a>,
    resources: Option<Obj<'a>>,
    media_box: Option<Obj<'a>>,
    crop_box: Option<Obj<'a>>,
    rotate: Option<Obj<'a>>,
}

/// What one inked page needs written.
struct Plan<'s> {
    strokes: &'s [Stroke<'s>],
    /// Text boxes and the font resource names (regular, bold, mono) when there are any.
    texts: &'s [crate::text_layout::PdfText],
    fonts: Option<[String; 3]>,
    /// Paper ruling and the displayed page size (points) it spans.
    paper: Option<(Paper, f64, f64)>,
    matrix: [f64; 6],
    oc_name: String,
    /// Resource name per alpha value this page switches to.
    gs_names: Vec<(u8, String)>,
    /// Images: resource name and placement.
    images: Vec<(String, PdfImagePlace)>,
}

#[derive(Clone, Copy)]
struct PdfImagePlace {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn build_update(pdf: &[u8], pages: &[&[Stroke<'_>]], papers: &[Option<Paper>], texts: &[&[crate::text_layout::PdfText]], images: &[&[PdfImage]]) -> Result<Vec<u8>> {
    let file = File::parse(pdf)?;
    let (root_num, root_gen) = match file.trailer.get("Root") {
        Some(Obj::Ref(n, g)) => (*n, *g),
        _ => return Err(bad("no catalog")),
    };
    let catalog = file.object(root_num)?.1.into_dict().ok_or_else(|| bad("catalog is not a dictionary"))?;
    let tree = page_tree(&file, &catalog)?;
    if tree.len() != pages.len() {
        return Err(Error::Invalid(format!("ink for {} pages, but the PDF has {}", pages.len(), tree.len())));
    }

    let mut next = file.size();
    let mut alloc = || {
        next += 1;
        next - 1
    };
    let (wrap_open, wrap_close, ocg) = (alloc(), alloc(), alloc());
    let mut gs_objects: BTreeMap<u8, u32> = BTreeMap::new();
    // The three standard fonts, written once if any page has text.
    let font_objects: Option<[u32; 3]> = texts.iter().any(|t| !t.is_empty()).then(|| [alloc(), alloc(), alloc()]);

    // Image objects to write: number, JPEG bytes, pixel size and components.
    let mut image_objects: Vec<(u32, &[u8], u32, u32, u8)> = Vec::new();

    // Plan every inked page: new resources, contents and the names its ink stream uses.
    let mut plans = Vec::new();
    let mut page_dicts = Vec::new();
    for (i, (node, &strokes)) in tree.into_iter().zip(pages).enumerate() {
        let paper = papers.get(i).copied().flatten().filter(|p| p.background != n0te_core::notebook::Background::Blank && p.spacing >= 2.0);
        let page_texts: &[crate::text_layout::PdfText] = texts.get(i).copied().unwrap_or(&[]);
        let page_images: Vec<(&PdfImage, (u32, u32, u8))> =
            images.get(i).copied().unwrap_or(&[]).iter().filter_map(|im| jpeg_info(&im.jpeg).filter(|j| j.2 == 1 || j.2 == 3).map(|j| (im, j))).collect();
        let mut alphas: Vec<u8> = strokes.iter().filter(|s| visible(s)).map(|s| (s.brush.color >> 24) as u8).collect();
        if alphas.is_empty() && paper.is_none() && page_texts.is_empty() && page_images.is_empty() {
            continue;
        }
        alphas.sort_unstable();
        alphas.dedup();
        if alphas == [255] {
            alphas.clear(); // the initial state is opaque: no ExtGState needed
        }

        let mut res = match resolve_opt(&file, node.resources.as_ref())? {
            Some(Obj::Dict(d)) => d,
            _ => Dict::default(),
        };
        let mut props = sub_dict(&file, &res, "Properties")?;
        let oc_name = unique_name(&props, OC_RESOURCE);
        props.set(&oc_name, Obj::Ref(ocg, 0));
        res.set("Properties", Obj::Dict(props));
        let mut gs_names = Vec::new();
        if !alphas.is_empty() {
            let mut ext = sub_dict(&file, &res, "ExtGState")?;
            for a in alphas {
                let obj = *gs_objects.entry(a).or_insert_with(&mut alloc);
                let name = unique_name(&ext, &format!("N0teA{a}"));
                ext.set(&name, Obj::Ref(obj, 0));
                gs_names.push((a, name));
            }
            res.set("ExtGState", Obj::Dict(ext));
        }
        let mut fonts = None;
        if let (Some(objs), false) = (font_objects, page_texts.is_empty()) {
            let mut fd = sub_dict(&file, &res, "Font")?;
            let names = ["N0teHelv", "N0teHelvB", "N0teMono"].map(|base| {
                let n = unique_name(&fd, base);
                fd.set(&n, Obj::Null);
                n
            });
            for (n, o) in names.iter().zip(objs) {
                fd.set(n, Obj::Ref(o, 0));
            }
            res.set("Font", Obj::Dict(fd));
            fonts = Some(names);
        }

        let mut image_names = Vec::new();
        if !page_images.is_empty() {
            let mut xo = sub_dict(&file, &res, "XObject")?;
            for (k, (im, (pw, ph, comps))) in page_images.iter().enumerate() {
                let num = alloc();
                let name = unique_name(&xo, &format!("N0teImg{k}"));
                xo.set(&name, Obj::Ref(num, 0));
                image_objects.push((num, &im.jpeg, *pw, *ph, *comps));
                image_names.push((name, PdfImagePlace { x: im.x, y: im.y, w: im.w, h: im.h }));
            }
            res.set("XObject", Obj::Dict(xo));
        }

        let ink_obj = alloc();
        let existing = contents(&file, node.dict.get("Contents"))?;
        let mut new_contents = Vec::with_capacity(existing.len() + 3);
        if !existing.is_empty() {
            new_contents.push(Obj::Ref(wrap_open, 0));
            new_contents.extend(existing);
            new_contents.push(Obj::Ref(wrap_close, 0));
        }
        new_contents.push(Obj::Ref(ink_obj, 0));

        let media = rect(&file, node.media_box.as_ref());
        let crop = rect(&file, node.crop_box.as_ref());
        let rotate = resolve_opt(&file, node.rotate.as_ref())?.and_then(|o| o.as_f64()).unwrap_or(0.0) as i64;
        let matrix = display_matrix(media, crop, rotate);
        let (pw, ph) = display_size(media, crop, rotate);

        let mut dict = node.dict;
        dict.set("Contents", Obj::Array(new_contents));
        dict.set("Resources", Obj::Dict(res));
        page_dicts.push((node.num, node.generation, dict, ink_obj));
        plans.push(Plan { strokes, texts: page_texts, fonts, paper: paper.map(|p| (p, pw, ph)), matrix, oc_name, gs_names, images: image_names });
    }

    if plans.is_empty() {
        return Ok(Vec::new()); // only invisible strokes
    }
    let streams = build_streams(&plans);

    let mut out = Out { buf: Vec::with_capacity(streams.iter().map(Vec::len).sum::<usize>() + 4096), base: pdf.len(), xref: Vec::new() };
    if !matches!(pdf.last(), Some(b'\n' | b'\r')) {
        out.buf.push(b'\n');
    }
    out.stream(wrap_open, b"q\n", false);
    out.stream(wrap_close, b"\nQ\n", false);
    let ocg_dict = Obj::Dict(Dict(vec![
        ("Type".as_bytes().into(), Obj::name("OCG")),
        ("Name".as_bytes().into(), Obj::Str(format!("({INK_LAYER_NAME})").into_bytes().into())),
        ("Intent".as_bytes().into(), Obj::name("View")),
        ("N0te".as_bytes().into(), Obj::num(1.0)),
    ]));
    out.object(ocg, 0, &ocg_dict);
    if let Some(objs) = font_objects {
        for (num, base) in objs.into_iter().zip(["Helvetica", "Helvetica-Bold", "Courier"]) {
            let f = Dict(vec![
                ("Type".as_bytes().into(), Obj::name("Font")),
                ("Subtype".as_bytes().into(), Obj::name("Type1")),
                ("BaseFont".as_bytes().into(), Obj::name(base)),
                ("Encoding".as_bytes().into(), Obj::name("WinAnsiEncoding")),
            ]);
            out.object(num, 0, &Obj::Dict(f));
        }
    }
    for &(num, data, w, h, comps) in &image_objects {
        out.image(num, data, w, h, comps);
    }
    for (&alpha, &num) in &gs_objects {
        let a = Obj::num(alpha as f64 / 255.0);
        let gs = Dict(vec![("Type".as_bytes().into(), Obj::name("ExtGState")), ("ca".as_bytes().into(), a.clone()), ("CA".as_bytes().into(), a)]);
        out.object(num, 0, &Obj::Dict(gs));
    }
    for ((num, generation, dict, ink_obj), data) in page_dicts.into_iter().zip(&streams) {
        out.stream(ink_obj, data, true);
        out.object(num, generation, &Obj::Dict(dict));
    }
    let catalog = with_ink_layer(&file, catalog, ocg)?;
    out.object(root_num, root_gen, &Obj::Dict(catalog));

    let mut trailer = file.trailer.clone();
    trailer.remove("XRefStm");
    trailer.set("Size", Obj::num(next as f64));
    trailer.set("Prev", Obj::num(file.startxref as f64));
    out.finish(&trailer);
    Ok(out.buf)
}

/// Turns n0te ink off: every n0te ink layer of the document (listed in `/OCProperties`, or only
/// referenced from a page, as happens when pages of an exported PDF were exported again) is
/// added to `/OCProperties /D /OFF`, so viewers and pdfium render the pages without it. Returns
/// the file with an incremental update, or unchanged if it has no n0te ink.
///
/// This is the building block for re-importing an exported PDF: show the original pages and
/// leave the ink to n0te.
pub fn hide_ink(mut pdf: Vec<u8>) -> Result<Vec<u8>> {
    let update = {
        let file = File::parse(&pdf)?;
        let (root_num, root_gen) = match file.trailer.get("Root") {
            Some(Obj::Ref(n, g)) => (*n, *g),
            _ => return Err(bad("no catalog")),
        };
        let mut catalog = file.object(root_num)?.1.into_dict().ok_or_else(|| bad("catalog is not a dictionary"))?;
        let mut props = match file.get(&catalog, "OCProperties")? {
            Some(Obj::Dict(d)) => d,
            _ => Dict::default(),
        };
        let listed = match file.get(&props, "OCGs")? {
            Some(Obj::Array(a)) => a,
            _ => Vec::new(),
        };
        let mut candidates: Vec<Obj> = listed.clone();
        for page in page_tree(&file, &catalog)? {
            if let Some(Obj::Dict(res)) = resolve_opt(&file, page.resources.as_ref())?
                && let Some(Obj::Dict(p)) = file.get(&res, "Properties")?
            {
                candidates.extend(p.0.into_iter().map(|(_, v)| v));
            }
        }
        let mut ink: Vec<Obj> = Vec::new();
        for c in candidates {
            if matches!(c, Obj::Ref(..)) && !ink.contains(&c) && is_ink_layer(&file, &c)? {
                ink.push(c);
            }
        }
        if ink.is_empty() {
            return Ok(pdf);
        }
        let mut ocgs = listed;
        let mut d = match file.get(&props, "D")? {
            Some(Obj::Dict(d)) => d,
            _ => Dict::default(),
        };
        let mut off = match file.get(&d, "OFF")? {
            Some(Obj::Array(a)) => a,
            _ => Vec::new(),
        };
        let on = match file.get(&d, "ON")? {
            Some(Obj::Array(a)) => a.into_iter().filter(|o| !ink.contains(o)).collect(),
            _ => Vec::new(),
        };
        for r in &ink {
            if !ocgs.contains(r) {
                ocgs.push(r.clone());
            }
            if !off.contains(r) {
                off.push(r.clone());
            }
        }
        if d.contains("ON") {
            d.set("ON", Obj::Array(on));
        }
        d.set("OFF", Obj::Array(off));
        props.set("OCGs", Obj::Array(ocgs));
        props.set("D", Obj::Dict(d));
        catalog.set("OCProperties", Obj::Dict(props));

        let mut out = Out { buf: Vec::new(), base: pdf.len(), xref: Vec::new() };
        if !matches!(pdf.last(), Some(b'\n' | b'\r')) {
            out.buf.push(b'\n');
        }
        out.object(root_num, root_gen, &Obj::Dict(catalog));
        let mut trailer = file.trailer.clone();
        trailer.remove("XRefStm");
        trailer.set("Prev", Obj::num(file.startxref as f64));
        out.finish(&trailer);
        out.buf
    };
    pdf.extend_from_slice(&update);
    Ok(pdf)
}

/// An OCG written by n0te: `/Name (n0te ink)` and the private `/N0te` key.
fn is_ink_layer<'a>(file: &File<'a>, o: &Obj<'a>) -> Result<bool> {
    let Obj::Dict(d) = file.resolve(o)? else { return Ok(false) };
    let name = format!("({INK_LAYER_NAME})");
    Ok(d.get("Type").is_some_and(|t| t.is_name("OCG")) && d.contains("N0te") && matches!(d.get("Name"), Some(Obj::Str(s)) if s.as_ref() == name.as_bytes()))
}

fn visible(s: &Stroke) -> bool {
    s.brush.color >> 24 != 0 && !s.points.is_empty()
}

fn page_tree<'a>(file: &File<'a>, catalog: &Dict<'a>) -> Result<Vec<PageNode<'a>>> {
    let Some(Obj::Ref(root, _)) = catalog.get("Pages") else {
        return Err(bad("catalog has no page tree"));
    };
    let mut out = Vec::new();
    walk(file, *root, Inherited::default(), 0, &mut HashSet::new(), &mut out)?;
    Ok(out)
}

#[derive(Clone, Default)]
struct Inherited<'a> {
    resources: Option<Obj<'a>>,
    media_box: Option<Obj<'a>>,
    crop_box: Option<Obj<'a>>,
    rotate: Option<Obj<'a>>,
}

fn walk<'a>(file: &File<'a>, num: u32, mut inh: Inherited<'a>, depth: u32, seen: &mut HashSet<u32>, out: &mut Vec<PageNode<'a>>) -> Result<()> {
    if depth > 64 || !seen.insert(num) {
        return Err(bad("page tree is cyclic or too deep"));
    }
    let (generation, obj) = file.object(num)?;
    let dict = obj.into_dict().ok_or_else(|| bad("page tree node is not a dictionary"))?;
    for (key, slot) in [("Resources", &mut inh.resources), ("MediaBox", &mut inh.media_box), ("CropBox", &mut inh.crop_box), ("Rotate", &mut inh.rotate)] {
        if let Some(v) = dict.get(key) {
            *slot = Some(v.clone());
        }
    }
    let is_tree = match dict.get("Type") {
        Some(t) => t.is_name("Pages"),
        None => dict.contains("Kids"),
    };
    if !is_tree {
        out.push(PageNode { num, generation, dict, resources: inh.resources, media_box: inh.media_box, crop_box: inh.crop_box, rotate: inh.rotate });
        return Ok(());
    }
    match file.get(&dict, "Kids")? {
        Some(Obj::Array(kids)) => {
            for kid in kids {
                let Obj::Ref(k, _) = kid else {
                    return Err(bad("page tree kid is not a reference"));
                };
                walk(file, k, inh.clone(), depth + 1, seen, out)?;
            }
            Ok(())
        }
        None => Ok(()),
        Some(_) => Err(bad("page tree /Kids is not an array")),
    }
}

fn resolve_opt<'a>(file: &File<'a>, o: Option<&Obj<'a>>) -> Result<Option<Obj<'a>>> {
    o.map(|o| file.resolve(o)).transpose()
}

/// A copy of resource sub-dictionary `key` (inline or indirect), or an empty one.
fn sub_dict<'a>(file: &File<'a>, res: &Dict<'a>, key: &str) -> Result<Dict<'a>> {
    Ok(match file.get(res, key)? {
        Some(Obj::Dict(d)) => d,
        _ => Dict::default(),
    })
}

/// `base`, or `base_1`, `base_2`, … if the name is taken in `dict`.
fn unique_name(dict: &Dict, base: &str) -> String {
    if !dict.contains(base) {
        return base.to_string();
    }
    (1..).map(|i| format!("{base}_{i}")).find(|n| !dict.contains(n)).expect("unbounded")
}

/// The content stream references of a page (`/Contents` may be a stream, an array of streams,
/// or a reference to such an array).
fn contents<'a>(file: &File<'a>, c: Option<&Obj<'a>>) -> Result<Vec<Obj<'a>>> {
    Ok(match c {
        None | Some(Obj::Null) => Vec::new(),
        Some(Obj::Array(a)) => a.iter().filter(|o| matches!(o, Obj::Ref(..))).cloned().collect(),
        Some(&Obj::Ref(n, g)) => match file.object(n) {
            Ok((_, Obj::Array(a))) => a.into_iter().filter(|o| matches!(o, Obj::Ref(..))).collect(),
            Ok(_) => vec![Obj::Ref(n, g)],
            Err(_) => Vec::new(), // dangling reference = no content
        },
        Some(_) => return Err(bad("bad /Contents")),
    })
}

/// A rectangle `[a b c d]`, normalized to (x0, y0, x1, y1).
fn rect<'a>(file: &File<'a>, o: Option<&Obj<'a>>) -> Option<[f64; 4]> {
    let Ok(Obj::Array(a)) = file.resolve(o?) else { return None };
    if a.len() != 4 {
        return None;
    }
    let mut v = [0.0; 4];
    for (slot, x) in v.iter_mut().zip(&a) {
        *slot = file.resolve(x).ok()?.as_f64()?;
    }
    Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
}

/// The matrix from page coordinates (points, top-left of the displayed page, y down) to user
/// space, laid out like pdfium does: crop box ∩ media box, turned by `/Rotate`.
/// Width and height of the page as displayed (crop box within the media box, after /Rotate).
pub(crate) fn display_size(media: Option<[f64; 4]>, crop: Option<[f64; 4]>, rotate: i64) -> (f64, f64) {
    let empty = |r: &[f64; 4]| r[0] >= r[2] || r[1] >= r[3];
    let media = media.filter(|r| !empty(r)).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let r = match crop {
        Some(c) if !empty(&c) => [c[0].max(media[0]), c[1].max(media[1]), c[2].min(media[2]), c[3].min(media[3])],
        _ => media,
    };
    let (w, h) = ((r[2] - r[0]).max(0.0), (r[3] - r[1]).max(0.0));
    let mut quarter = (rotate / 90) % 4;
    if quarter < 0 {
        quarter += 4;
    }
    if quarter % 2 == 1 { (h, w) } else { (w, h) }
}

pub(crate) fn display_matrix(media: Option<[f64; 4]>, crop: Option<[f64; 4]>, rotate: i64) -> [f64; 6] {
    let empty = |r: &[f64; 4]| r[0] >= r[2] || r[1] >= r[3];
    let media = media.filter(|r| !empty(r)).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let [x0, y0, x1, y1] = match crop {
        Some(c) if !empty(&c) => {
            let r = [c[0].max(media[0]), c[1].max(media[1]), c[2].min(media[2]), c[3].min(media[3])];
            if r[0] > r[2] || r[1] > r[3] { [0.0; 4] } else { r }
        }
        _ => media,
    };
    // pdfium: (Rotate / 90) % 4 with C integer division, negative turns wrap around.
    let mut quarter = (rotate / 90) % 4;
    if quarter < 0 {
        quarter += 4;
    }
    match quarter {
        0 => [1.0, 0.0, 0.0, -1.0, x0, y1],
        1 => [0.0, 1.0, 1.0, 0.0, x0, y0],
        2 => [-1.0, 0.0, 0.0, 1.0, x1, y0],
        _ => [0.0, -1.0, -1.0, 0.0, x1, y1],
    }
}

/// The catalog with the ink OCG added to `/OCProperties` (created if missing).
fn with_ink_layer<'a>(file: &File<'a>, mut catalog: Dict<'a>, ocg: u32) -> Result<Dict<'a>> {
    let mut props = match file.get(&catalog, "OCProperties")? {
        Some(Obj::Dict(d)) => d,
        _ => Dict::default(),
    };
    push_ref(file, &mut props, "OCGs", ocg)?;
    let mut d = match file.get(&props, "D")? {
        Some(Obj::Dict(d)) => d,
        _ => Dict::default(),
    };
    push_ref(file, &mut d, "Order", ocg)?;
    if d.get("BaseState").is_some_and(|b| !b.is_name("ON")) {
        push_ref(file, &mut d, "ON", ocg)?;
    }
    props.set("D", Obj::Dict(d));
    catalog.set("OCProperties", Obj::Dict(props));
    Ok(catalog)
}

/// Appends a reference to the array `key` of `dict` (inline or indirect; created if missing).
fn push_ref<'a>(file: &File<'a>, dict: &mut Dict<'a>, key: &str, num: u32) -> Result<()> {
    let mut items = match file.get(dict, key)? {
        Some(Obj::Array(a)) => a,
        _ => Vec::new(),
    };
    items.push(Obj::Ref(num, 0));
    dict.set(key, Obj::Array(items));
    Ok(())
}

/// Compressed ink streams of all plans, in order. Pages are independent, so they are built on
/// several threads when there are several pages.
fn build_streams(plans: &[Plan]) -> Vec<Vec<u8>> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8).min(plans.len());
    if threads <= 1 {
        return plans.iter().map(ink_stream).collect();
    }
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, Vec<u8>)>> = Mutex::new(Vec::with_capacity(plans.len()));
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(plan) = plans.get(i) else { break };
                    let data = ink_stream(plan);
                    done.lock().unwrap_or_else(|p| p.into_inner()).push((i, data));
                }
            });
        }
    });
    let mut done = done.into_inner().unwrap_or_else(|p| p.into_inner());
    done.sort_unstable_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, d)| d).collect()
}

/// The (compressed) ink stream of one page.
fn ink_stream(plan: &Plan) -> Vec<u8> {
    let raw = ink_content(plan);
    let mut z = ZlibEncoder::new(Vec::with_capacity(raw.len() / 3), Compression::new(FLATE_LEVEL));
    z.write_all(&raw).expect("writing to a Vec cannot fail");
    z.finish().expect("writing to a Vec cannot fail")
}

/// The page's images, in page coordinates after `cm` (top-left origin, y down): the unit square
/// is mapped flipped, so the picture stands upright.
fn image_content(out: &mut Vec<u8>, plan: &Plan) {
    for (name, p) in &plan.images {
        out.extend_from_slice(b"q\n");
        for v in plan.matrix {
            write_num(out, v, 4);
            out.push(b' ');
        }
        out.extend_from_slice(b"cm\n");
        for v in [p.w as f64, 0.0, 0.0, -(p.h as f64), p.x as f64, (p.y + p.h) as f64] {
            write_num(out, v, 3);
            out.push(b' ');
        }
        out.extend_from_slice(b"cm\n/");
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(b" Do\nQ\n");
    }
}

/// The paper ruling (outside the ink layer), in page coordinates after `cm`. Colours match the
/// app: light blue-grey lines, grey dots, a light red Cornell frame.
fn paper_content(out: &mut Vec<u8>, plan: &Plan) {
    use n0te_core::notebook::Background;
    let Some((paper, w, h)) = plan.paper else { return };
    let step = paper.spacing as f64;
    out.extend_from_slice(b"q\n");
    for v in plan.matrix {
        write_num(out, v, 4);
        out.push(b' ');
    }
    out.extend_from_slice(b"cm\n0.839 0.863 0.902 RG 0.5 w\n");
    let line = |out: &mut Vec<u8>, x0: f64, y0: f64, x1: f64, y1: f64| {
        write_num(out, x0, 2); out.push(b' '); write_num(out, y0, 2); out.extend_from_slice(b" m ");
        write_num(out, x1, 2); out.push(b' '); write_num(out, y1, 2); out.extend_from_slice(b" l\n");
    };
    match paper.background {
        Background::Lines => {
            let mut y = step * 2.0;
            while y < h { line(out, 0.0, y, w, y); y += step; }
            out.extend_from_slice(b"S\n");
        }
        Background::Grid => {
            let mut x = step;
            while x < w { line(out, x, 0.0, x, h); x += step; }
            let mut y = step;
            while y < h { line(out, 0.0, y, w, y); y += step; }
            out.extend_from_slice(b"S\n");
        }
        Background::Dots => {
            out.extend_from_slice(b"0.761 0.788 0.831 rg\n");
            let mut x = step;
            while x < w {
                let mut y = step;
                while y < h {
                    write_num(out, x - 0.65, 2); out.push(b' '); write_num(out, y - 0.65, 2); out.extend_from_slice(b" 1.3 1.3 re\n");
                    y += step;
                }
                x += step;
            }
            out.extend_from_slice(b"f\n");
        }
        Background::Cornell => {
            let mut y = 90.0;
            while y < h - 150.0 { line(out, 0.0, y, w, y); y += step; }
            out.extend_from_slice(b"S\n0.937 0.600 0.612 RG 0.8 w\n");
            line(out, 150.0, 70.0, 150.0, h - 150.0);
            line(out, 0.0, h - 150.0, w, h - 150.0);
            line(out, 0.0, 70.0, w, 70.0);
            out.extend_from_slice(b"S\n");
        }
        Background::Blank | Background::Text => {}
    }
    out.extend_from_slice(b"Q\n");
}

/// The uncompressed content of a page's ink stream (see the module docs).
fn ink_content(plan: &Plan) -> Vec<u8> {
    let mut out = Vec::with_capacity(plan.strokes.iter().map(|s| s.points.len() * 30 + 64).sum::<usize>() + 128);
    paper_content(&mut out, plan);
    image_content(&mut out, plan);
    if !plan.strokes.iter().any(visible) && plan.texts.is_empty() {
        return out;
    }
    out.extend_from_slice(b"/OC /");
    out.extend_from_slice(plan.oc_name.as_bytes());
    out.extend_from_slice(b" BDC\nq\n");
    for v in plan.matrix {
        write_num(&mut out, v, 4);
        out.push(b' ');
    }
    out.extend_from_slice(b"cm\n");
    text_content(&mut out, plan);
    let (mut color, mut alpha) = (None, 255u8);
    for s in plan.strokes.iter().filter(|s| visible(s)) {
        let poly = ink::outline(s.points, &s.brush);
        if poly.len() < 6 {
            continue;
        }
        let rgb = s.brush.color & 0x00FF_FFFF;
        if color != Some(rgb) {
            for shift in [16, 8, 0] {
                write_num(&mut out, ((rgb >> shift) & 0xFF) as f64 / 255.0, 3);
                out.push(b' ');
            }
            out.extend_from_slice(b"rg\n");
            color = Some(rgb);
        }
        let a = (s.brush.color >> 24) as u8;
        if a != alpha
            && let Some((_, name)) = plan.gs_names.iter().find(|(x, _)| *x == a)
        {
            out.push(b'/');
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(b" gs\n");
            alpha = a;
        }
        for (i, xy) in poly.chunks_exact(2).enumerate() {
            write_num(&mut out, xy[0] as f64, 2);
            out.push(b' ');
            write_num(&mut out, xy[1] as f64, 2);
            out.extend_from_slice(if i == 0 { b" m\n" } else { b" l\n" });
        }
        out.extend_from_slice(b"f\n");
    }
    out.extend_from_slice(b"Q\nEMC\n");
    out
}

/// Text boxes (page coordinates, y down after `cm`): each line its own text object with a
/// flipped text matrix so glyphs stand upright.
fn text_content(out: &mut Vec<u8>, plan: &Plan) {
    use crate::text_layout::{Face, layout, pdf_string};
    let Some(names) = &plan.fonts else { return };
    for t in plan.texts {
        for line in layout(t) {
            let font = match line.face {
                Face::Regular => &names[0],
                Face::Bold => &names[1],
                Face::Mono => &names[2],
            };
            out.extend_from_slice(if line.grey { b"0.54 0.54 0.57 rg\nBT /" } else { b"0.067 0.067 0.078 rg\nBT /" });
            out.extend_from_slice(font.as_bytes());
            out.extend_from_slice(b" 1 Tf ");
            for v in [line.size as f64, 0.0, 0.0, -line.size as f64, line.x as f64, line.baseline as f64] {
                write_num(out, v, 3);
                out.push(b' ');
            }
            out.extend_from_slice(b"Tm ");
            out.extend_from_slice(&pdf_string(&line.bytes));
            out.extend_from_slice(b" Tj ET\n");
        }
    }
}

/// The update being written: objects and their offsets in the final file.
struct Out {
    buf: Vec<u8>,
    /// Length of the original file (offsets in `buf` are relative to its end).
    base: usize,
    xref: Vec<(u32, u16, usize)>,
}

impl Out {
    fn begin(&mut self, num: u32, generation: u16) {
        self.xref.push((num, generation, self.base + self.buf.len()));
        write_uint(&mut self.buf, num as u64);
        self.buf.push(b' ');
        write_uint(&mut self.buf, generation as u64);
        self.buf.extend_from_slice(b" obj\n");
    }

    fn object(&mut self, num: u32, generation: u16, o: &Obj) {
        self.begin(num, generation);
        o.write(&mut self.buf);
        self.buf.extend_from_slice(b"\nendobj\n");
    }

    /// A JPEG as an image XObject (DCTDecode: the bytes go in unchanged).
    fn image(&mut self, num: u32, jpeg: &[u8], w: u32, h: u32, comps: u8) {
        self.begin(num, 0);
        self.buf.extend_from_slice(b"<</Type/XObject/Subtype/Image/Width ");
        write_uint(&mut self.buf, w as u64);
        self.buf.extend_from_slice(b"/Height ");
        write_uint(&mut self.buf, h as u64);
        self.buf.extend_from_slice(if comps == 1 { b"/ColorSpace/DeviceGray".as_slice() } else { b"/ColorSpace/DeviceRGB".as_slice() });
        self.buf.extend_from_slice(b"/BitsPerComponent 8/Filter/DCTDecode/Length ");
        write_uint(&mut self.buf, jpeg.len() as u64);
        self.buf.extend_from_slice(b">>stream\n");
        self.buf.extend_from_slice(jpeg);
        self.buf.extend_from_slice(b"\nendstream\nendobj\n");
    }

    fn stream(&mut self, num: u32, data: &[u8], flate: bool) {
        self.begin(num, 0);
        self.buf.extend_from_slice(b"<</Length ");
        write_uint(&mut self.buf, data.len() as u64);
        if flate {
            self.buf.extend_from_slice(b"/Filter/FlateDecode");
        }
        self.buf.extend_from_slice(b">>stream\n");
        self.buf.extend_from_slice(data);
        self.buf.extend_from_slice(b"\nendstream\nendobj\n");
    }

    /// Cross-reference section (one subsection per run of consecutive numbers) and trailer.
    fn finish(&mut self, trailer: &Dict) {
        let at = self.base + self.buf.len();
        self.xref.sort_unstable_by_key(|e| e.0);
        self.buf.extend_from_slice(b"xref\n");
        let mut i = 0;
        while i < self.xref.len() {
            let mut j = i + 1;
            while j < self.xref.len() && self.xref[j].0 == self.xref[j - 1].0 + 1 {
                j += 1;
            }
            let _ = writeln!(self.buf, "{} {}", self.xref[i].0, j - i);
            for &(_, generation, off) in &self.xref[i..j] {
                let _ = write!(self.buf, "{off:010} {generation:05} n\r\n");
            }
            i = j;
        }
        self.buf.extend_from_slice(b"trailer\n");
        trailer.write(&mut self.buf);
        let _ = write!(self.buf, "\nstartxref\n{at}\n%%EOF\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use n0te_core::ink::{Brush, InkPoint};

    /// A hand-written PDF with inherited attributes: the Pages node carries /Resources (indirect,
    /// with an ExtGState already named like ours), /Rotate 90 and a /CropBox; page 2 overrides the
    /// media box and has its content as a reference to an array.
    fn handmade() -> Vec<u8> {
        let objs: Vec<String> = vec![
            "<</Type/Catalog/Pages 2 0 R>>".into(),
            "<</Type/Pages/Kids[3 0 R 4 0 R]/Count 2/Resources 5 0 R/MediaBox[0 0 600 800]/CropBox[10 20 590 780]/Rotate 90>>".into(),
            "<</Type/Page/Parent 2 0 R/Contents 6 0 R>>".into(),
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 300]/Contents 7 0 R>>".into(),
            "<</ExtGState<</N0teA128 8 0 R>>/Font<<>>>>".into(),
            "<</Length 9>>stream\n0 0 m 1 1\nendstream".into(),
            "[6 0 R 6 0 R]".into(),
            "<</Type/ExtGState/ca 0.3>>".into(),
        ];
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut offs = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offs.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let x = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f\r\n", objs.len() + 1).as_bytes());
        for o in offs {
            pdf.extend_from_slice(format!("{o:010} 00000 n\r\n").as_bytes());
        }
        pdf.extend_from_slice(format!("trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
        pdf
    }

    fn dot(x: f32, y: f32, color: u32) -> (Vec<InkPoint>, Brush) {
        (vec![InkPoint { x, y, pressure: 0.5, t_ms: 0 }], Brush { width: 4.0, thinning: 0.0, color, kind: 0 })
    }

    #[test]
    fn inherited_attributes_and_existing_names() {
        let (p1, b1) = dot(5.0, 5.0, 0x80FF0000);
        let (p2, b2) = dot(7.0, 7.0, 0xFF000000);
        let s1 = [Stroke { points: &p1, brush: b1 }, Stroke { points: &p2, brush: b2 }];
        let s2 = [Stroke { points: &p2, brush: b2 }];
        let pdf = handmade();
        let base_len = pdf.len();
        let out = add_ink(pdf, &[&s1, &s2]).unwrap();
        let f = File::parse(&out).unwrap();
        assert!(f.startxref > base_len, "an incremental update was appended");
        let cat = f.resolve(f.trailer.get("Root").unwrap()).unwrap().into_dict().unwrap();
        let tree = page_tree(&f, &cat).unwrap();
        assert_eq!(tree.len(), 2);

        // Page 1: resources copied from the Pages node, our alpha name made unique, content wrapped.
        let p = &tree[0];
        let res = f.resolve(p.dict.get("Resources").unwrap()).unwrap().into_dict().unwrap();
        let ext = f.get(&res, "ExtGState").unwrap().unwrap().into_dict().unwrap();
        assert_eq!(ext.get("N0teA128"), Some(&Obj::Ref(8, 0)), "existing entry kept");
        assert!(ext.contains("N0teA128_1") && ext.contains("N0teA255"));
        assert!(res.contains("Font"));
        let Some(Obj::Array(c)) = p.dict.get("Contents") else { panic!() };
        assert_eq!(c.len(), 4, "q, original, Q, ink: {c:?}");
        // Page 2: content array behind a reference is flattened.
        let Some(Obj::Array(c)) = tree[1].dict.get("Contents") else { panic!() };
        assert_eq!(c.len(), 5);
        // The shared resources object itself is untouched.
        let shared = f.object(5).unwrap().1.into_dict().unwrap();
        assert!(!shared.contains("Properties"));

        // Catalog lists the layer.
        let ocp = f.get(&cat, "OCProperties").unwrap().unwrap().into_dict().unwrap();
        let Some(Obj::Array(ocgs)) = ocp.get("OCGs") else { panic!() };
        let ocg = f.resolve(&ocgs[0]).unwrap().into_dict().unwrap();
        assert_eq!(ocg.get("Name"), Some(&Obj::Str(b"(n0te ink)"[..].into())));
    }

    #[test]
    fn matrix_follows_rotation_and_crop() {
        let crop = Some([10.0, 20.0, 590.0, 780.0]);
        let media = Some([0.0, 0.0, 600.0, 800.0]);
        let apply = |m: [f64; 6], x: f64, y: f64| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
        // Display top-left corner goes to: r0 user top-left, r90 bottom-left, r180 bottom-right, r270 top-right.
        assert_eq!(apply(display_matrix(media, crop, 0), 0.0, 0.0), (10.0, 780.0));
        assert_eq!(apply(display_matrix(media, crop, 90), 0.0, 0.0), (10.0, 20.0));
        assert_eq!(apply(display_matrix(media, crop, 180), 0.0, 0.0), (590.0, 20.0));
        assert_eq!(apply(display_matrix(media, crop, 270), 0.0, 0.0), (590.0, 780.0));
        assert_eq!(display_matrix(media, crop, -90), display_matrix(media, crop, 270));
        assert_eq!(display_matrix(media, crop, 450), display_matrix(media, crop, 90));
        // A crop box sticking out of the media box is clipped; no boxes = US Letter.
        assert_eq!(apply(display_matrix(media, Some([-50.0, 0.0, 700.0, 900.0]), 0), 0.0, 0.0), (0.0, 800.0));
        assert_eq!(apply(display_matrix(None, None, 0), 0.0, 0.0), (0.0, 792.0));
    }

    #[test]
    fn page_count_must_match_and_no_ink_is_a_no_op() {
        let (p, b) = dot(1.0, 1.0, 0xFF000000);
        let s = [Stroke { points: &p, brush: b }];
        assert!(add_ink(handmade(), &[&s]).is_err());
        assert_eq!(add_ink(handmade(), &[&[], &[]]).unwrap(), handmade());
        // Fully transparent strokes are dropped: nothing to write.
        let (p, b) = dot(1.0, 1.0, 0x00FF0000);
        let s = [Stroke { points: &p, brush: b }];
        assert_eq!(add_ink(handmade(), &[&s, &[]]).unwrap(), handmade());
    }

    #[test]
    fn content_stream_shape() {
        let (p1, b1) = dot(5.0, 5.0, 0x80FF0000);
        let (p2, b2) = dot(7.0, 7.0, 0xFF000000);
        let strokes = [Stroke { points: &p2, brush: b2 }, Stroke { points: &p1, brush: b1 }, Stroke { points: &p2, brush: b2 }];
        let plan = Plan { strokes: &strokes, texts: &[], fonts: None, paper: None, matrix: [1.0, 0.0, 0.0, -1.0, 0.0, 842.0], oc_name: "N0teInk".into(), gs_names: vec![(128, "A".into()), (255, "B".into())], images: vec![] };
        let s = String::from_utf8(ink_content(&plan)).unwrap();
        assert!(s.starts_with("/OC /N0teInk BDC\nq\n1 0 0 -1 0 842 cm\n0 0 0 rg\n"), "{s}");
        assert!(s.ends_with("f\nQ\nEMC\n"));
        assert_eq!(s.matches(" rg\n").count(), 3);
        assert_eq!(s.matches(" gs\n").count(), 2, "to 128 and back to 255: {s}");
        assert_eq!(s.matches(" m\n").count(), 3);
        assert!(s.contains("1 0 0 rg\n/A gs\n"));
    }
}
