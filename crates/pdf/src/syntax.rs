//! Just enough PDF syntax to read the page tree of a file pdfium wrote and to append an
//! incremental update to it (see [`crate::ink_writer`]).
//!
//! Objects are parsed zero-copy. Tokens that are written back unchanged (numbers, strings, names)
//! keep their raw bytes, so a dictionary we rewrite keeps every value exactly as it was without
//! decoding strings or reformatting numbers. Stream data is never read.
//!
//! Only classic cross-reference tables are supported (what pdfium's `FPDF_SaveAsCopy` writes);
//! cross-reference streams and object streams are refused with an error.

use std::borrow::Cow;
use std::collections::HashSet;

use crate::{Error, Result};

/// Nesting limit for arrays and dictionaries (guards the recursion on hostile input).
const MAX_DEPTH: u32 = 64;
/// Largest object number accepted (PDF's own implementation limit is 8 388 607).
const MAX_OBJECTS: usize = 1 << 23;

fn bad(msg: impl Into<String>) -> Error {
    Error::Invalid(format!("pdf structure: {}", msg.into()))
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Obj<'a> {
    Null,
    Bool(bool),
    /// A number, as written.
    Num(Cow<'a, [u8]>),
    /// A literal `( … )` or hex `< … >` string, as written (delimiters included).
    Str(Cow<'a, [u8]>),
    /// A name without the leading `/`, as written (`#xx` escapes not decoded).
    Name(Cow<'a, [u8]>),
    Array(Vec<Obj<'a>>),
    Dict(Dict<'a>),
    Ref(u32, u16),
}

impl<'a> Obj<'a> {
    pub(crate) fn name(s: &str) -> Obj<'a> {
        Obj::Name(Cow::Owned(s.as_bytes().to_vec()))
    }

    pub(crate) fn num(v: f64) -> Obj<'a> {
        let mut b = Vec::new();
        write_num(&mut b, v, 4);
        Obj::Num(Cow::Owned(b))
    }

    pub(crate) fn as_f64(&self) -> Option<f64> {
        match self {
            Obj::Num(raw) => std::str::from_utf8(raw).ok()?.parse::<f64>().ok().filter(|v| v.is_finite()),
            _ => None,
        }
    }

    pub(crate) fn is_name(&self, want: &str) -> bool {
        matches!(self, Obj::Name(raw) if name_eq(raw, want))
    }

    pub(crate) fn into_dict(self) -> Option<Dict<'a>> {
        match self {
            Obj::Dict(d) => Some(d),
            _ => None,
        }
    }

    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        match self {
            Obj::Null => out.extend_from_slice(b"null"),
            Obj::Bool(true) => out.extend_from_slice(b"true"),
            Obj::Bool(false) => out.extend_from_slice(b"false"),
            Obj::Num(raw) | Obj::Str(raw) => out.extend_from_slice(raw),
            Obj::Name(raw) => {
                out.push(b'/');
                out.extend_from_slice(raw);
            }
            Obj::Array(items) => {
                out.push(b'[');
                for (i, o) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(b' ');
                    }
                    o.write(out);
                }
                out.push(b']');
            }
            Obj::Dict(d) => d.write(out),
            Obj::Ref(n, g) => {
                write_uint(out, *n as u64);
                out.push(b' ');
                write_uint(out, *g as u64);
                out.extend_from_slice(b" R");
            }
        }
    }
}

/// A dictionary, entries in file order. Keys are raw names without the `/`.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Dict<'a>(pub(crate) Vec<(Cow<'a, [u8]>, Obj<'a>)>);

impl<'a> Dict<'a> {
    pub(crate) fn get(&self, key: &str) -> Option<&Obj<'a>> {
        self.0.iter().find(|(k, _)| name_eq(k, key)).map(|(_, v)| v)
    }

    pub(crate) fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Replaces the value of `key`, or appends the entry.
    pub(crate) fn set(&mut self, key: &str, value: Obj<'a>) {
        match self.0.iter_mut().find(|(k, _)| name_eq(k, key)) {
            Some(entry) => entry.1 = value,
            None => self.0.push((Cow::Owned(key.as_bytes().to_vec()), value)),
        }
    }

    pub(crate) fn remove(&mut self, key: &str) {
        self.0.retain(|(k, _)| !name_eq(k, key));
    }

    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"<<");
        for (k, v) in &self.0 {
            out.push(b'/');
            out.extend_from_slice(k);
            out.push(b' ');
            v.write(out);
        }
        out.extend_from_slice(b">>");
    }
}

/// Compares a raw name (possibly with `#xx` escapes) with a plain one.
pub(crate) fn name_eq(raw: &[u8], want: &str) -> bool {
    if !raw.contains(&b'#') {
        return raw == want.as_bytes();
    }
    let mut decoded = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'#'
            && i + 2 < raw.len()
            && let (Some(h), Some(l)) = (hex(raw[i + 1]), hex(raw[i + 2]))
        {
            decoded.push(h << 4 | l);
            i += 3;
        } else {
            decoded.push(raw[i]);
            i += 1;
        }
    }
    decoded == want.as_bytes()
}

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'\0')
}

fn is_delim(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

fn is_regular(c: u8) -> bool {
    !is_ws(c) && !is_delim(c)
}

pub(crate) struct Lexer<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    pub(crate) fn at(b: &'a [u8], i: usize) -> Self {
        Lexer { b, i: i.min(b.len()) }
    }

    fn skip_ws(&mut self) {
        loop {
            while self.i < self.b.len() && is_ws(self.b[self.i]) {
                self.i += 1;
            }
            if self.i < self.b.len() && self.b[self.i] == b'%' {
                while self.i < self.b.len() && !matches!(self.b[self.i], b'\n' | b'\r') {
                    self.i += 1;
                }
            } else {
                return;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn regular_token(&mut self) -> &'a [u8] {
        let start = self.i;
        while self.i < self.b.len() && is_regular(self.b[self.i]) {
            self.i += 1;
        }
        &self.b[start..self.i]
    }

    /// Consumes `kw` if it is the next token.
    fn keyword(&mut self, kw: &[u8]) -> bool {
        let save = self.i;
        self.skip_ws();
        if self.regular_token() == kw {
            return true;
        }
        self.i = save;
        false
    }

    /// Consumes an unsigned integer if it is the next token.
    fn uint(&mut self) -> Option<u64> {
        let save = self.i;
        self.skip_ws();
        let tok = self.regular_token();
        if !tok.is_empty() && tok.len() <= 18 && tok.iter().all(u8::is_ascii_digit) {
            return Some(tok.iter().fold(0u64, |v, d| v * 10 + (d - b'0') as u64));
        }
        self.i = save;
        None
    }

    pub(crate) fn object(&mut self, depth: u32) -> Result<Obj<'a>> {
        if depth > MAX_DEPTH {
            return Err(bad("objects nested too deeply"));
        }
        self.skip_ws();
        let c = self.peek().ok_or_else(|| bad("unexpected end of file"))?;
        match c {
            b'/' => {
                self.i += 1;
                Ok(Obj::Name(Cow::Borrowed(self.regular_token())))
            }
            b'(' => self.literal_string(),
            b'<' if self.b.get(self.i + 1) == Some(&b'<') => {
                self.i += 2;
                let mut d = Dict::default();
                loop {
                    self.skip_ws();
                    if self.b[self.i..].starts_with(b">>") {
                        self.i += 2;
                        return Ok(Obj::Dict(d));
                    }
                    if self.peek() != Some(b'/') {
                        return Err(bad("dictionary key is not a name"));
                    }
                    self.i += 1;
                    let key = self.regular_token();
                    let value = self.object(depth + 1)?;
                    d.0.push((Cow::Borrowed(key), value));
                }
            }
            b'<' => {
                let end = self.b[self.i..].iter().position(|&c| c == b'>').ok_or_else(|| bad("unterminated hex string"))?;
                let s = &self.b[self.i..=self.i + end];
                self.i += end + 1;
                Ok(Obj::Str(Cow::Borrowed(s)))
            }
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some(b']') {
                        self.i += 1;
                        return Ok(Obj::Array(items));
                    }
                    items.push(self.object(depth + 1)?);
                }
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => {
                let tok = self.regular_token();
                if !tok.iter().all(|c| matches!(c, b'+' | b'-' | b'.' | b'0'..=b'9')) {
                    return Err(bad(format!("bad number {:?}", String::from_utf8_lossy(tok))));
                }
                if tok.iter().all(u8::is_ascii_digit) && tok.len() <= 9 {
                    let save = self.i;
                    if let Some(g) = self.uint()
                        && g <= u16::MAX as u64
                        && self.keyword(b"R")
                    {
                        let n = tok.iter().fold(0u32, |v, d| v * 10 + (d - b'0') as u32);
                        return Ok(Obj::Ref(n, g as u16));
                    }
                    self.i = save;
                }
                Ok(Obj::Num(Cow::Borrowed(tok)))
            }
            c if is_regular(c) => match self.regular_token() {
                b"true" => Ok(Obj::Bool(true)),
                b"false" => Ok(Obj::Bool(false)),
                b"null" => Ok(Obj::Null),
                tok => Err(bad(format!("unexpected keyword {:?}", String::from_utf8_lossy(tok)))),
            },
            c => Err(bad(format!("unexpected byte {:?}", c as char))),
        }
    }

    fn literal_string(&mut self) -> Result<Obj<'a>> {
        let start = self.i;
        self.i += 1;
        let mut depth = 1u32;
        while self.i < self.b.len() {
            let c = self.b[self.i];
            self.i += 1;
            match c {
                b'\\' => self.i += 1,
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(Obj::Str(Cow::Borrowed(&self.b[start..self.i])));
                    }
                }
                _ => {}
            }
        }
        Err(bad("unterminated string"))
    }
}

/// A parsed file: where its objects are and its trailer.
pub(crate) struct File<'a> {
    pub(crate) bytes: &'a [u8],
    /// Byte offset and generation per object number (in-use entries only).
    offsets: Vec<Option<(usize, u16)>>,
    /// The newest trailer.
    pub(crate) trailer: Dict<'a>,
    /// Offset of the newest cross-reference section (the update's `/Prev`).
    pub(crate) startxref: usize,
}

impl<'a> File<'a> {
    pub(crate) fn parse(bytes: &'a [u8]) -> Result<Self> {
        let tail = &bytes[bytes.len().saturating_sub(1024)..];
        let at = tail.windows(9).rposition(|w| w == b"startxref").ok_or_else(|| bad("no startxref"))?;
        let mut lx = Lexer::at(bytes, bytes.len() - tail.len() + at + 9);
        let startxref = lx.uint().ok_or_else(|| bad("bad startxref"))? as usize;

        let mut offsets: Vec<Option<(usize, u16)>> = Vec::new();
        let mut trailer: Option<Dict<'a>> = None;
        let mut seen = HashSet::new();
        let mut section = Some(startxref);
        while let Some(pos) = section.take() {
            if pos >= bytes.len() || !seen.insert(pos) {
                return Err(bad("bad cross-reference offset"));
            }
            let mut lx = Lexer::at(bytes, pos);
            if !lx.keyword(b"xref") {
                return Err(bad("cross-reference streams are not supported"));
            }
            while !lx.keyword(b"trailer") {
                let (start, count) = match (lx.uint(), lx.uint()) {
                    (Some(s), Some(c)) => (s as usize, c as usize),
                    _ => return Err(bad("bad cross-reference table")),
                };
                if start.saturating_add(count) > MAX_OBJECTS {
                    return Err(bad("too many objects"));
                }
                if offsets.len() < start + count {
                    offsets.resize(start + count, None);
                }
                for k in 0..count {
                    let (off, generation) = match (lx.uint(), lx.uint()) {
                        (Some(o), Some(g)) => (o as usize, g.min(u16::MAX as u64) as u16),
                        _ => return Err(bad("bad cross-reference entry")),
                    };
                    lx.skip_ws();
                    let kind = lx.regular_token();
                    // Older sections only fill in what newer ones did not mention.
                    if kind == b"n" && offsets[start + k].is_none() && start + k > 0 {
                        offsets[start + k] = Some((off, generation));
                    } else if kind != b"n" && kind != b"f" {
                        return Err(bad("bad cross-reference entry type"));
                    }
                }
            }
            let t = lx.object(0)?.into_dict().ok_or_else(|| bad("trailer is not a dictionary"))?;
            section = t.get("Prev").and_then(Obj::as_f64).map(|p| p as usize);
            if trailer.is_none() {
                trailer = Some(t);
            }
        }
        let trailer = trailer.ok_or_else(|| bad("no trailer"))?;
        if trailer.contains("Encrypt") {
            return Err(bad("encrypted output is not supported"));
        }
        Ok(File { bytes, offsets, trailer, startxref })
    }

    /// One past the highest object number in use (the trailer's `/Size`, checked).
    pub(crate) fn size(&self) -> u32 {
        let declared = self.trailer.get("Size").and_then(Obj::as_f64).unwrap_or(0.0) as usize;
        declared.max(self.offsets.len()).min(MAX_OBJECTS) as u32
    }

    /// Reads object `num`. Returns its generation and value (a stream comes back as its
    /// dictionary).
    pub(crate) fn object(&self, num: u32) -> Result<(u16, Obj<'a>)> {
        let (off, generation) = self.offsets.get(num as usize).copied().flatten().ok_or_else(|| bad(format!("object {num} missing")))?;
        let mut lx = Lexer::at(self.bytes, off);
        match (lx.uint(), lx.uint(), lx.keyword(b"obj")) {
            (Some(n), Some(_), true) if n == num as u64 => Ok((generation, lx.object(0)?)),
            _ => Err(bad(format!("object {num} not found at its offset"))),
        }
    }

    /// Follows references (a missing object is `null`, as the PDF spec says).
    pub(crate) fn resolve(&self, o: &Obj<'a>) -> Result<Obj<'a>> {
        let mut cur = o.clone();
        for _ in 0..32 {
            match cur {
                Obj::Ref(n, _) => match self.object(n) {
                    Ok((_, v)) => cur = v,
                    Err(_) => return Ok(Obj::Null),
                },
                v => return Ok(v),
            }
        }
        Err(bad("reference chain too long"))
    }

    /// The value of `key` in `dict`, resolved; `None` if absent or `null`.
    pub(crate) fn get(&self, dict: &Dict<'a>, key: &str) -> Result<Option<Obj<'a>>> {
        match dict.get(key) {
            None => Ok(None),
            Some(o) => Ok(Some(self.resolve(o)?).filter(|v| *v != Obj::Null)),
        }
    }
}

/// Writes a non-negative integer.
pub(crate) fn write_uint(out: &mut Vec<u8>, mut v: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    out.extend_from_slice(&buf[i..]);
}

/// Writes `v` rounded to `decimals` (≤ 6) places in the shortest form: no exponent, no trailing
/// zeros, no `-0`.
pub(crate) fn write_num(out: &mut Vec<u8>, v: f64, decimals: u32) {
    let scale = 10u64.pow(decimals);
    let q = (v * scale as f64).round();
    if !q.is_finite() || q.abs() >= 9.0e15 {
        out.push(b'0');
        return;
    }
    let q = q as i64;
    if q < 0 {
        out.push(b'-');
    }
    let a = q.unsigned_abs();
    write_uint(out, a / scale);
    let mut frac = a % scale;
    if frac != 0 {
        let mut digits = decimals;
        while frac % 10 == 0 {
            frac /= 10;
            digits -= 1;
        }
        out.push(b'.');
        let mut buf = [b'0'; 6];
        let mut i = digits as usize;
        while frac > 0 {
            i -= 1;
            buf[i] = b'0' + (frac % 10) as u8;
            frac /= 10;
        }
        out.extend_from_slice(&buf[..digits as usize]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Obj<'_> {
        Lexer::at(s.as_bytes(), 0).object(0).unwrap()
    }

    fn round_trip(s: &str) -> String {
        let mut out = Vec::new();
        parse(s).write(&mut out);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn parses_and_writes_back_objects() {
        assert_eq!(round_trip("<</Type/Page/Contents 12 0 R/Kids[1 0 R 2 0 R]>>"), "<</Type /Page/Contents 12 0 R/Kids [1 0 R 2 0 R]>>");
        assert_eq!(round_trip("[ -1.5 +3 .25 4. (a (nested) \\) string) <AbCd> /N#20ame true false null ]"), "[-1.5 +3 .25 4. (a (nested) \\) string) <AbCd> /N#20ame true false null]");
        assert_eq!(round_trip("<< % comment\n /A [1 2 3] /B << /C (x) >> >>"), "<</A [1 2 3]/B <</C (x)>>>>");
        // "1 0" not followed by R stays two numbers.
        assert_eq!(parse("[1 0 2]"), Obj::Array(vec![Obj::Num(b"1"[..].into()), Obj::Num(b"0"[..].into()), Obj::Num(b"2"[..].into())]));
        assert_eq!(parse("7 0 R"), Obj::Ref(7, 0));
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        for s in ["", "<<", "<< /A >>", "(unterminated", "[1 2", "<</A 1", "{", "endobj", "<<1 2>>", "<abc", "1-2x", &"[".repeat(500)] {
            assert!(Lexer::at(s.as_bytes(), 0).object(0).is_err(), "{s:?}");
        }
    }

    #[test]
    fn names_compare_with_escapes() {
        assert!(name_eq(b"Resources", "Resources"));
        assert!(name_eq(b"Re#73ources", "Resources"));
        assert!(!name_eq(b"Res", "Resources"));
        assert!(!name_eq(b"A#", "A#x"));
    }

    #[test]
    fn dict_set_replaces_or_appends() {
        let mut d = parse("<</A 1/B 2>>").into_dict().unwrap();
        d.set("B", Obj::num(3.0));
        d.set("C", Obj::name("X"));
        d.remove("A");
        let mut out = Vec::new();
        d.write(&mut out);
        assert_eq!(out, b"<</B 3/C /X>>");
    }

    #[test]
    fn numbers_are_short() {
        let s = |v: f64, d: u32| {
            let mut o = Vec::new();
            write_num(&mut o, v, d);
            String::from_utf8(o).unwrap()
        };
        assert_eq!(s(12.0, 2), "12");
        assert_eq!(s(12.5, 2), "12.5");
        assert_eq!(s(-0.004, 2), "0");
        assert_eq!(s(-0.006, 2), "-0.01");
        assert_eq!(s(0.05, 2), "0.05");
        assert_eq!(s(841.89, 2), "841.89");
        assert_eq!(s(1.0 / 255.0 * 128.0, 3), "0.502");
        assert_eq!(s(f64::NAN, 2), "0");
    }

    #[test]
    fn reads_a_classic_xref_file_with_updates() {
        // A base file plus one incremental update that replaces object 2.
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let o1 = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<</Type/Catalog/Pages 2 0 R>>\nendobj\n");
        let o2 = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<</Type/Pages/Kids[]/Count 0>>\nendobj\n");
        let x1 = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 3\n0000000000 65535 f\r\n{o1:010} 00000 n\r\n{o2:010} 00000 n\r\ntrailer\n<</Size 3/Root 1 0 R>>\nstartxref\n{x1}\n%%EOF\n").as_bytes());
        let o2b = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<</Type/Pages/Kids[]/Count 0/New true>>\nendobj\n");
        let x2 = pdf.len();
        pdf.extend_from_slice(format!("xref\n2 1\n{o2b:010} 00000 n\r\ntrailer\n<</Size 3/Root 1 0 R/Prev {x1}>>\nstartxref\n{x2}\n%%EOF\n").as_bytes());

        let f = File::parse(&pdf).unwrap();
        assert_eq!(f.startxref, x2);
        assert_eq!(f.size(), 3);
        let pages = f.object(2).unwrap().1.into_dict().unwrap();
        assert_eq!(pages.get("New"), Some(&Obj::Bool(true)));
        let cat = f.resolve(f.trailer.get("Root").unwrap()).unwrap().into_dict().unwrap();
        assert!(cat.get("Type").unwrap().is_name("Catalog"));
        assert!(f.object(3).is_err());
        assert_eq!(f.resolve(&Obj::Ref(9, 0)).unwrap(), Obj::Null);
    }

    #[test]
    fn refuses_xref_streams_and_broken_files() {
        assert!(File::parse(b"%PDF-1.7\n1 0 obj\n<</Type/XRef>>stream\nendstream\nendobj\nstartxref\n9\n%%EOF").is_err());
        assert!(File::parse(b"%PDF-1.7\nno trailer here").is_err());
        assert!(File::parse(b"startxref\n999999\n%%EOF").is_err());
        assert!(File::parse(b"xref\n0 99999999999\ntrailer<<>>startxref\n0\n%%EOF").is_err());
    }
}
