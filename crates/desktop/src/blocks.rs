//! Flowing text page = block editor. A vertical list of blocks (paragraph, heading, bullet,
//! code, formula); each block is one iced `text_editor`. Structure editing is done here:
//!
//! - Enter splits the block at the cursor (Shift+Enter = line break inside the block; in code
//!   and formula blocks Enter is a line break and Ctrl+Enter leaves the block).
//! - Backspace at the start turns a heading/bullet/code block back into a paragraph, or merges
//!   a paragraph into the previous block. Delete at the end pulls the next block in.
//! - Up/Down on the first/last visual line, Left/Right at the edges move to the neighbour.
//! - Markdown at the start of a paragraph converts it: "# ", "## ", "### ", "- ", "* ",
//!   "```lang" + Enter (or "```"), "$$".
//! - Undo/redo (Ctrl+Z, Ctrl+Shift+Z / Ctrl+Y) over the whole page; iced's editor has none.

use std::time::{Duration, Instant};

use iced::keyboard::{Key, key::Named};
use iced::widget::text_editor;
use iced::widget::text_editor::{Action, Binding, Content, Cursor, Edit, KeyPress, Motion, Position};
use iced::widget::{Space, column, container, mouse_area, row, text};
use iced::{Alignment, Element, Font, Length, Padding, Task, font, widget};

use crate::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Paragraph,
    Heading(u8),
    Bullet,
    Numbered,
    /// A checklist item (ticked or not).
    Todo(bool),
    Quote,
    Code { lang: String },
    Formula,
}

impl Kind {
    /// List-like blocks: Enter continues them, Enter on an empty one ends the list.
    pub fn is_list(&self) -> bool {
        matches!(self, Kind::Bullet | Kind::Numbered | Kind::Todo(_) | Kind::Quote)
    }
    /// Consecutive blocks of the same class are one Markdown list.
    fn list_class(&self) -> u8 {
        match self {
            Kind::Bullet => 1,
            Kind::Numbered => 2,
            Kind::Todo(_) => 3,
            Kind::Quote => 4,
            _ => 0,
        }
    }
    /// Text with inline Markdown (formatted while typing; formulas drawn when not edited).
    fn is_text(&self) -> bool {
        !matches!(self, Kind::Code { .. } | Kind::Formula)
    }
    /// Code and formula blocks keep Enter as a line break.
    pub fn is_multiline(&self) -> bool {
        matches!(self, Kind::Code { .. } | Kind::Formula)
    }
    fn from_prefix(prefix: &str) -> Kind {
        match prefix {
            "#" => Kind::Heading(1),
            "##" => Kind::Heading(2),
            "###" => Kind::Heading(3),
            "-" => Kind::Bullet,
            "1." => Kind::Numbered,
            "[ ]" => Kind::Todo(false),
            "[x]" => Kind::Todo(true),
            ">" => Kind::Quote,
            "$$" => Kind::Formula,
            p if p.starts_with("```") => Kind::Code { lang: p[3..].to_string() },
            _ => Kind::Paragraph,
        }
    }
}

pub struct Block {
    pub id: u64,
    pub kind: Kind,
    pub content: Content,
}

impl Block {
    pub fn text(&self) -> String {
        // `Content::text` adds a trailing newline for the last line; strip it.
        let mut t = self.content.text();
        if t.ends_with('\n') {
            t.pop();
        }
        t
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Action(u64, Action),
    Enter(u64),
    ExitBlock(u64),
    Backspace(u64),
    Delete(u64),
    Up(u64),
    Down(u64),
    Left(u64),
    Right(u64),
    Undo,
    Redo,
    /// The toolbar: paragraph kind of the block being edited (again: back to a paragraph).
    SetKind(Kind),
    /// The toolbar / Ctrl+B, Ctrl+I: bold, italic, code, formula around the selection.
    Wrap(&'static str),
    ToggleTodo(u64),
    /// A click on a block shown rendered (with formulas): edit it.
    FocusEnd(u64),
}

#[derive(Clone)]
struct Snapshot {
    blocks: Vec<(u64, Kind, String)>,
    focus: Option<(u64, Cursor)>,
}

pub struct Doc {
    pub blocks: Vec<Block>,
    /// The block that last received input (iced owns the real focus state).
    pub focus: Option<u64>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Coalescing of typing into one undo step.
    last_edit: Option<(u64, EditClass, Instant)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditClass {
    Insert,
    Delete,
    Other,
}

static NEXT_BLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn editor_id(block: u64) -> widget::Id {
    widget::Id::from(format!("blk-{block}"))
}

impl Doc {
    pub fn new(items: &[(&str, &str)]) -> Self {
        let mut doc = Doc { blocks: Vec::new(), focus: None, undo: Vec::new(), redo: Vec::new(), last_edit: None };
        for (prefix, body) in items {
            let id = doc.alloc();
            doc.blocks.push(Block { id, kind: Kind::from_prefix(prefix), content: Content::with_text(body) });
        }
        if doc.blocks.is_empty() {
            let id = doc.alloc();
            doc.blocks.push(Block { id, kind: Kind::Paragraph, content: Content::new() });
        }
        doc
    }

    /// Blocks from Markdown (a text page's body): headings, bullets, ``` code, $$ formulas,
    /// paragraphs (lines up to a blank line, joined with line breaks).
    pub fn from_markdown(md: &str) -> Self {
        let lines: Vec<&str> = md.split('\n').collect();
        let mut items: Vec<(String, String)> = Vec::new();
        let mut i = 0;
        let special = |l: &str| {
            let t = l.trim_start();
            t.starts_with("# ") || t.starts_with("## ") || t.starts_with("### ") || t.starts_with("- ") || t.starts_with("* ") || t.starts_with("> ") || numbered(t).is_some() || t.starts_with("```") || t.trim() == "$$"
        };
        while i < lines.len() {
            let line = lines[i];
            let t = line.trim_start();
            if t.trim().is_empty() {
                i += 1;
                continue;
            }
            if let Some(lang) = t.strip_prefix("```") {
                let mut body = Vec::new();
                i += 1;
                while i < lines.len() && lines[i].trim() != "```" {
                    body.push(lines[i]);
                    i += 1;
                }
                items.push((format!("```{}", lang.trim()), body.join("\n")));
                i += 1;
                continue;
            }
            if t.trim() == "$$" {
                let mut body = Vec::new();
                i += 1;
                while i < lines.len() && lines[i].trim() != "$$" {
                    body.push(lines[i]);
                    i += 1;
                }
                items.push(("$$".into(), body.join("\n")));
                i += 1;
                continue;
            }
            let heading = [("### ", "###"), ("## ", "##"), ("# ", "#")].into_iter().find_map(|(p, k)| t.strip_prefix(p).map(|rest| (k, rest)));
            if let Some((k, rest)) = heading {
                items.push((k.into(), rest.to_string()));
                i += 1;
                continue;
            }
            let todo = [("- [ ] ", "[ ]"), ("* [ ] ", "[ ]"), ("- [x] ", "[x]"), ("- [X] ", "[x]"), ("* [x] ", "[x]")].into_iter().find_map(|(p, k)| t.strip_prefix(p).map(|rest| (k, rest)));
            if let Some((k, rest)) = todo {
                items.push((k.into(), rest.to_string()));
                i += 1;
                continue;
            }
            if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
                items.push(("-".into(), rest.to_string()));
                i += 1;
                continue;
            }
            if let Some(rest) = numbered(t) {
                items.push(("1.".into(), rest.to_string()));
                i += 1;
                continue;
            }
            if let Some(rest) = t.strip_prefix("> ") {
                items.push((">".into(), rest.to_string()));
                i += 1;
                continue;
            }
            let mut para = vec![line];
            i += 1;
            while i < lines.len() && !lines[i].trim().is_empty() && !special(lines[i]) {
                para.push(lines[i]);
                i += 1;
            }
            items.push((String::new(), para.join("\n")));
        }
        let refs: Vec<(&str, &str)> = items.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        Doc::new(&refs)
    }

    /// The page as Markdown (inverse of [`Doc::from_markdown`]); bullets in a row stay a list.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let mut prev: Option<&Kind> = None;
        let mut n = 0;
        for b in &self.blocks {
            let text = b.text();
            if let Some(p) = prev {
                out.push_str(if p.is_list() && p.list_class() == b.kind.list_class() { "\n" } else { "\n\n" });
            }
            n = if b.kind == Kind::Numbered { if prev == Some(&Kind::Numbered) { n + 1 } else { 1 } } else { 0 };
            match &b.kind {
                Kind::Paragraph => out.push_str(&text),
                Kind::Heading(n) => {
                    out.push_str(&"#".repeat(*n as usize));
                    out.push(' ');
                    out.push_str(&text);
                }
                Kind::Bullet => {
                    out.push_str("- ");
                    out.push_str(&text);
                }
                Kind::Numbered => out.push_str(&format!("{n}. {text}")),
                Kind::Todo(done) => out.push_str(&format!("- [{}] {text}", if *done { "x" } else { " " })),
                Kind::Quote => out.push_str(&text.split('\n').map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n")),
                Kind::Code { lang } => out.push_str(&format!("```{lang}\n{text}\n```")),
                Kind::Formula => out.push_str(&format!("$$\n{text}\n$$")),
            }
            prev = Some(&b.kind);
        }
        out
    }

    /// A new block id, unique in the whole app: two text pages can be open at once (one in each
    /// pane of the split), and focusing a block goes by its widget id.
    fn alloc(&mut self) -> u64 {
        NEXT_BLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    pub fn index(&self, id: u64) -> Option<usize> {
        self.blocks.iter().position(|b| b.id == id)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            blocks: self.blocks.iter().map(|b| (b.id, b.kind.clone(), b.text())).collect(),
            focus: self.focus.and_then(|id| self.index(id).map(|i| (id, self.blocks[i].content.cursor()))),
        }
    }

    /// Records an undo step before a change. Typing in one block coalesces until a pause,
    /// a switch between inserting and deleting, or a word boundary.
    fn checkpoint(&mut self, block: u64, class: EditClass, boundary: bool) {
        let now = Instant::now();
        let coalesce = matches!(self.last_edit, Some((b, c, t)) if b == block && c == class && class != EditClass::Other && !boundary && now - t < Duration::from_millis(1200));
        if !coalesce {
            self.undo.push(self.snapshot());
            if self.undo.len() > 500 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_edit = Some((block, class, now));
    }

    fn restore(&mut self, s: Snapshot) -> Task<Msg> {
        self.blocks = s.blocks.into_iter().map(|(id, kind, t)| Block { id, kind, content: Content::with_text(&t) }).collect();
        self.last_edit = None;
        match s.focus {
            Some((id, cursor)) if self.index(id).is_some() => self.focus_at(id, cursor),
            _ => Task::none(),
        }
    }

    /// Focuses block `id` with the cursor at `cursor` (e.g. after opening the page).
    pub fn focus_block(&mut self, id: u64, cursor: Cursor) -> Task<Msg> {
        self.focus_at(id, cursor)
    }

    /// Focuses a block's editor and puts its cursor at `cursor`.
    fn focus_at(&mut self, id: u64, cursor: Cursor) -> Task<Msg> {
        if let Some(i) = self.index(id) {
            let b = &mut self.blocks[i];
            b.content.move_to(clamp_cursor(&b.content, cursor));
        }
        self.focus = Some(id);
        widget::operation::focus(editor_id(id))
    }

    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Action(id, action) => self.action(id, action),
            Msg::Enter(id) => self.enter(id),
            Msg::ExitBlock(id) => self.exit_block(id),
            Msg::Backspace(id) => self.backspace(id),
            Msg::Delete(id) => self.delete(id),
            Msg::Up(id) => self.vertical(id, Motion::Up),
            Msg::Down(id) => self.vertical(id, Motion::Down),
            Msg::Left(id) => self.horizontal(id, false),
            Msg::Right(id) => self.horizontal(id, true),
            Msg::Undo => match self.undo.pop() {
                Some(s) => {
                    self.redo.push(self.snapshot());
                    self.restore(s)
                }
                None => Task::none(),
            },
            Msg::Redo => match self.redo.pop() {
                Some(s) => {
                    self.undo.push(self.snapshot());
                    self.restore(s)
                }
                None => Task::none(),
            },
            Msg::SetKind(kind) => self.set_kind(kind),
            Msg::Wrap(m) => self.wrap(m),
            Msg::ToggleTodo(id) => {
                if let Some(i) = self.index(id)
                    && let Kind::Todo(done) = self.blocks[i].kind
                {
                    self.checkpoint(id, EditClass::Other, true);
                    self.blocks[i].kind = Kind::Todo(!done);
                }
                Task::none()
            }
            Msg::FocusEnd(id) => {
                let Some(i) = self.index(id) else { return Task::none() };
                let end = end_position(&self.blocks[i].text());
                self.focus_at(id, Cursor { position: end, selection: None })
            }
        }
    }

    /// The paragraph kind of the block being edited; the same kind again: back to a paragraph.
    fn set_kind(&mut self, kind: Kind) -> Task<Msg> {
        let Some(id) = self.focus.or_else(|| self.blocks.last().map(|b| b.id)) else { return Task::none() };
        let Some(i) = self.index(id) else { return Task::none() };
        self.checkpoint(id, EditClass::Other, true);
        let cur = self.blocks[i].content.cursor();
        let same = self.blocks[i].kind == kind || matches!((&self.blocks[i].kind, &kind), (Kind::Todo(_), Kind::Todo(_)));
        let kind = if same && kind != Kind::Paragraph { Kind::Paragraph } else { kind };
        let text = self.blocks[i].text();
        self.convert(i, kind, &text, cur.position)
    }

    /// Bold / italic / code / formula like in a word processor: around the selection (again:
    /// off); at the cursor a pair to type into (again right away: gone again).
    fn wrap(&mut self, m: &'static str) -> Task<Msg> {
        let Some(id) = self.focus else { return Task::none() };
        let Some(i) = self.index(id) else { return Task::none() };
        self.checkpoint(id, EditClass::Other, true);
        wrap_in(&mut self.blocks[i].content, m);
        let cur = self.blocks[i].content.cursor();
        self.focus_at(id, cur)
    }

    fn action(&mut self, id: u64, action: Action) -> Task<Msg> {
        let Some(i) = self.index(id) else { return Task::none() };
        self.focus = Some(id);
        if let Action::Edit(edit) = &action {
            let (class, boundary) = match edit {
                Edit::Insert(c) => (EditClass::Insert, c.is_whitespace()),
                Edit::Backspace | Edit::Delete => (EditClass::Delete, false),
                _ => (EditClass::Other, true),
            };
            self.checkpoint(id, class, boundary);
        }
        let is_insert = matches!(action, Action::Edit(Edit::Insert(_)));
        self.blocks[i].content.perform(action);
        if is_insert { self.markdown_shortcut(i) } else { Task::none() }
    }

    /// "# ", "## ", "### ", "- ", "* " and "$$" at the start of a paragraph convert it.
    fn markdown_shortcut(&mut self, i: usize) -> Task<Msg> {
        let b = &self.blocks[i];
        let para = b.kind == Kind::Paragraph;
        if !para && b.kind != Kind::Bullet {
            return Task::none();
        }
        let cur = b.content.cursor();
        if cur.position.line != 0 {
            return Task::none();
        }
        let first = b.content.line(0).map(|l| l.text.to_string()).unwrap_or_default();
        let head = &first[..cur.position.column.min(first.len())];
        let kind = match (head, para) {
            ("# ", true) => Kind::Heading(1),
            ("## ", true) => Kind::Heading(2),
            ("### ", true) => Kind::Heading(3),
            ("- " | "* ", true) => Kind::Bullet,
            ("1. ", true) => Kind::Numbered,
            ("> ", true) => Kind::Quote,
            ("$$", true) => Kind::Formula,
            // In a bullet: "[ ] " makes it a checklist item (typing "- [ ] " ends up here).
            ("[ ] " | "[] ", _) => Kind::Todo(false),
            ("[x] ", _) => Kind::Todo(true),
            _ => return Task::none(),
        };
        let rest = self.blocks[i].text()[head.len()..].to_string();
        self.convert(i, kind, &rest, Position { line: 0, column: 0 })
    }

    fn convert(&mut self, i: usize, kind: Kind, text: &str, at: Position) -> Task<Msg> {
        let id = self.blocks[i].id;
        self.blocks[i] = Block { id, kind, content: Content::with_text(text) };
        // The widget type can change (plain vs highlighted editor), so refocus explicitly.
        self.focus_at(id, Cursor { position: at, selection: None })
    }

    fn enter(&mut self, id: u64) -> Task<Msg> {
        let Some(i) = self.index(id) else { return Task::none() };
        self.checkpoint(id, EditClass::Other, true);
        let b = &self.blocks[i];
        let text = b.text();
        // "```lang" + Enter in a paragraph opens a code block.
        if b.kind == Kind::Paragraph && text.starts_with("```") && !text.contains('\n') && text[3..].chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '#') {
            let lang = text[3..].to_string();
            return self.convert(i, Kind::Code { lang }, "", Position { line: 0, column: 0 });
        }
        // Enter on an empty list item leaves the list.
        if b.kind.is_list() && text.is_empty() {
            return self.convert(i, Kind::Paragraph, "", Position { line: 0, column: 0 });
        }
        let off = byte_offset(&b.content, b.content.cursor().position);
        let (head, tail) = text.split_at(off);
        let (head, tail) = (head.to_string(), tail.to_string());
        let next_kind = match &b.kind {
            Kind::Todo(_) => Kind::Todo(false),
            k if k.is_list() => k.clone(),
            Kind::Heading(_) if !tail.is_empty() => b.kind.clone(),
            _ => Kind::Paragraph,
        };
        let kind = b.kind.clone();
        self.blocks[i] = Block { id, kind, content: Content::with_text(&head) };
        let new_id = self.alloc();
        self.blocks.insert(i + 1, Block { id: new_id, kind: next_kind, content: Content::with_text(&tail) });
        self.focus_at(new_id, Cursor { position: Position { line: 0, column: 0 }, selection: None })
    }

    /// Ctrl+Enter: a new paragraph below (the way out of code and formula blocks).
    fn exit_block(&mut self, id: u64) -> Task<Msg> {
        let Some(i) = self.index(id) else { return Task::none() };
        self.checkpoint(id, EditClass::Other, true);
        let new_id = self.alloc();
        self.blocks.insert(i + 1, Block { id: new_id, kind: Kind::Paragraph, content: Content::new() });
        self.focus_at(new_id, Cursor { position: Position { line: 0, column: 0 }, selection: None })
    }

    fn backspace(&mut self, id: u64) -> Task<Msg> {
        let Some(i) = self.index(id) else { return Task::none() };
        let cur = self.blocks[i].content.cursor();
        let at_start = cur.selection.is_none() && cur.position == Position { line: 0, column: 0 };
        if !at_start {
            return self.action(id, Action::Edit(Edit::Backspace));
        }
        if self.blocks[i].kind != Kind::Paragraph {
            self.checkpoint(id, EditClass::Other, true);
            let text = self.blocks[i].text();
            return self.convert(i, Kind::Paragraph, &text, Position { line: 0, column: 0 });
        }
        if i == 0 {
            return Task::none();
        }
        self.checkpoint(id, EditClass::Other, true);
        self.merge_into_previous(i)
    }

    fn merge_into_previous(&mut self, i: usize) -> Task<Msg> {
        let tail = self.blocks.remove(i).text();
        let prev = &mut self.blocks[i - 1];
        let head = prev.text();
        let join = end_position(&head);
        let kind = prev.kind.clone();
        let pid = prev.id;
        *prev = Block { id: pid, kind, content: Content::with_text(&(head + &tail)) };
        self.focus_at(pid, Cursor { position: join, selection: None })
    }

    fn delete(&mut self, id: u64) -> Task<Msg> {
        let Some(i) = self.index(id) else { return Task::none() };
        let b = &self.blocks[i];
        let cur = b.content.cursor();
        let at_end = cur.selection.is_none() && cur.position == end_position(&b.text());
        if !at_end || i + 1 >= self.blocks.len() {
            return self.action(id, Action::Edit(Edit::Delete));
        }
        self.checkpoint(id, EditClass::Other, true);
        self.merge_into_previous(i + 1)
    }

    /// Up/Down: move inside the block; if the cursor cannot move (first/last visual line),
    /// go to the neighbouring block. Works with wrapped lines because the editor decides.
    fn vertical(&mut self, id: u64, motion: Motion) -> Task<Msg> {
        let Some(i) = self.index(id) else { return Task::none() };
        self.last_edit = None;
        let before = self.blocks[i].content.cursor();
        self.blocks[i].content.perform(Action::Move(motion));
        let after = self.blocks[i].content.cursor();
        if after.position != before.position {
            return Task::none();
        }
        let column = before.position.column;
        match motion {
            Motion::Up if i > 0 => {
                let prev = &self.blocks[i - 1];
                let last = prev.content.line_count().saturating_sub(1);
                let pid = prev.id;
                self.focus_at(pid, Cursor { position: Position { line: last, column }, selection: None })
            }
            Motion::Down if i + 1 < self.blocks.len() => {
                let nid = self.blocks[i + 1].id;
                self.focus_at(nid, Cursor { position: Position { line: 0, column }, selection: None })
            }
            _ => Task::none(),
        }
    }

    fn horizontal(&mut self, id: u64, right: bool) -> Task<Msg> {
        let Some(i) = self.index(id) else { return Task::none() };
        self.last_edit = None;
        let b = &self.blocks[i];
        let cur = b.content.cursor();
        let edge = if right { end_position(&b.text()) } else { Position { line: 0, column: 0 } };
        if cur.selection.is_some() || cur.position != edge {
            self.blocks[i].content.perform(Action::Move(if right { Motion::Right } else { Motion::Left }));
            return Task::none();
        }
        if right && i + 1 < self.blocks.len() {
            let nid = self.blocks[i + 1].id;
            self.focus_at(nid, Cursor { position: Position { line: 0, column: 0 }, selection: None })
        } else if !right && i > 0 {
            let prev = &self.blocks[i - 1];
            let (pid, end) = (prev.id, end_position(&prev.text()));
            self.focus_at(pid, Cursor { position: end, selection: None })
        } else {
            Task::none()
        }
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let mut n = 0;
        let mut prev_numbered = false;
        let blocks: Vec<Element<'_, Msg>> = self
            .blocks
            .iter()
            .map(|b| {
                n = if b.kind == Kind::Numbered { if prev_numbered { n + 1 } else { 1 } } else { 0 };
                prev_numbered = b.kind == Kind::Numbered;
                self.block_view(b, n)
            })
            .collect();
        column(blocks).spacing(4).width(Length::Fill).into()
    }

    fn block_view<'a>(&'a self, b: &'a Block, number: usize) -> Element<'a, Msg> {
        let id = b.id;
        let kind = b.kind.clone();
        let multiline = kind.is_multiline();
        let bindings = move |kp: KeyPress| key_binding(id, multiline, kp);
        let (font, size, line) = match &b.kind {
            Kind::Heading(1) => (theme::SANS_BOLD, 30.0, 1.25),
            Kind::Heading(2) => (theme::SANS_BOLD, 22.0, 1.3),
            Kind::Heading(_) => (theme::SANS_MEDIUM, 18.0, 1.35),
            Kind::Code { .. } | Kind::Formula => (theme::MONO, 13.5, 1.5),
            Kind::Quote => (Font { style: font::Style::Italic, ..theme::SANS }, theme::BODY, 1.5),
            _ => (theme::SANS, theme::BODY, 1.5),
        };
        let focused = self.focus == Some(id);
        let only_empty = self.blocks.len() == 1 && b.content.is_empty();
        let focused_empty = self.focus == Some(id) && b.content.is_empty();
        let placeholder = match &b.kind {
            Kind::Paragraph if only_empty || focused_empty => "Schreiben … ( # Überschrift   - Liste   ``` Code   $$ Formel )",
            Kind::Heading(_) => "Überschrift",
            Kind::Bullet | Kind::Numbered => "Listenpunkt",
            Kind::Todo(_) => "Aufgabe",
            Kind::Quote => "Zitat",
            Kind::Formula => "LaTeX, z. B. \\int_a^b f(x)\\,dx",
            Kind::Code { .. } => "Code",
            _ => "",
        };
        let editor = text_editor(&b.content)
            .id(editor_id(id))
            .placeholder(placeholder)
            .on_action(move |a| Msg::Action(id, a))
            .key_binding(bindings)
            .font(font)
            .size(size)
            .line_height(line)
            .padding(Padding::from([2, 0]))
            .style(theme::editor_on_paper);
        // Inline marks (**, *, `, $): shown formatted without the marks unless being edited.
        let has_marks = b.kind.is_text() && !crate::mdstyle::pairs(&b.text()).is_empty();
        let editor: Element<'a, Msg> = match &b.kind {
            Kind::Code { lang } => editor.highlight(if lang.is_empty() { "txt" } else { lang }, iced::highlighter::Theme::InspiredGitHub).into(),
            Kind::Formula => editor.into(),
            // Formatted text, not being edited: drawn like the finished page (click edits it).
            _ if has_marks && !focused => rendered_text(b, font, size),
            _ => editor.highlight_with::<crate::mdstyle::MdInline>((), crate::mdstyle::format).into(),
        };
        // Every block has the same shape (gutter + body) so the widget tree stays stable.
        let gutter: Element<'a, Msg> = match &b.kind {
            Kind::Bullet => container(bullet())
                .padding(Padding { top: 11.0, left: 8.0, ..Padding::ZERO })
                .into(),
            Kind::Heading(n) => text(format!("H{n}")).font(theme::MONO).size(10).color(theme::PAPER_RULE).into(),
            Kind::Numbered => container(text(format!("{number}.")).font(theme::MONO_BOLD).size(12).color(theme::p().accent)).padding(Padding { top: 4.0, ..Padding::ZERO }).into(),
            Kind::Todo(done) => iced::widget::button(text(if *done { "☑" } else { "☐" }).size(17).color(theme::p().accent))
                .padding(Padding { top: 1.0, left: 2.0, ..Padding::ZERO })
                .on_press(Msg::ToggleTodo(id))
                .style(iced::widget::button::text)
                .into(),
            Kind::Quote => container(Space::new().width(3).height(22))
                .padding(Padding { top: 3.0, left: 10.0, ..Padding::ZERO })
                .style(|_| container::Style { background: Some(theme::PAPER_RULE.into()), ..Default::default() })
                .into(),
            _ => Space::new().into(),
        };
        let body: Element<'a, Msg> = match &b.kind {
            Kind::Code { lang } => container(column![
                row![Space::new().width(Length::Fill), text(if lang.is_empty() { "CODE".to_string() } else { lang.to_uppercase() }).font(theme::MONO).size(10).color(theme::PAPER_MUTED)],
                editor
            ])
            .padding(Padding { top: 6.0, right: 12.0, bottom: 10.0, left: 14.0 })
            .style(theme::code_box)
            .into(),
            // Not being edited: the formula itself (click edits it).
            Kind::Formula if !focused && !b.content.is_empty() => mouse_area({
                let c = container(formula_view(&b.text(), true, 17.0)).padding(Padding { top: 10.0, right: 12.0, bottom: 10.0, left: 12.0 });
                if crate::settings::formula_center() { c.center_x(Length::Fill) } else { c.width(Length::Fill) }
            })
            .on_press(Msg::FocusEnd(id))
            .interaction(iced::mouse::Interaction::Text)
            .into(),
            Kind::Formula => container(column![
                row![
                    text("$$").font(theme::MONO_BOLD).size(10).color(theme::p().accent),
                    Space::new().width(Length::Fill),
                    text("FORMEL · LaTeX").font(theme::MONO).size(10).color(theme::PAPER_MUTED)
                ],
                editor,
                container(formula_view(&b.text(), true, 17.0)).center_x(Length::Fill).padding(Padding { top: 8.0, ..Padding::ZERO }),
            ])
            .padding(Padding { top: 6.0, right: 12.0, bottom: 10.0, left: 14.0 })
            .style(theme::formula_box)
            .into(),
            Kind::Heading(1) => container(editor).padding(Padding { top: 18.0, bottom: 6.0, ..Padding::ZERO }).into(),
            Kind::Heading(_) => container(editor).padding(Padding { top: 12.0, bottom: 2.0, ..Padding::ZERO }).into(),
            _ => container(editor).into(),
        };
        row![container(gutter).width(28), body].into()
    }
}

/// Our key bindings on top of iced's defaults. Only the focused editor reacts.
fn key_binding(id: u64, multiline: bool, kp: KeyPress) -> Option<Binding<Msg>> {
    if !matches!(kp.status, text_editor::Status::Focused { .. }) {
        return None;
    }
    let m = kp.modifiers;
    let ctrl = m.command();
    if ctrl && !m.alt() {
        match kp.key.to_latin(kp.physical_key) {
            Some('z') if m.shift() => return Some(Binding::Custom(Msg::Redo)),
            Some('z') => return Some(Binding::Custom(Msg::Undo)),
            Some('y') => return Some(Binding::Custom(Msg::Redo)),
            Some('c' | 'x' | 'v' | 'a') => return Binding::from_key_press(kp),
            _ => {}
        }
    }
    match kp.key.as_ref() {
        Key::Named(Named::Enter) if ctrl => Some(Binding::Custom(Msg::ExitBlock(id))),
        Key::Named(Named::Enter) if m.shift() || multiline => Some(Binding::Enter),
        Key::Named(Named::Enter) => Some(Binding::Custom(Msg::Enter(id))),
        // iced has no word deletion; build it from a selection.
        Key::Named(Named::Backspace) if ctrl => Some(Binding::Sequence(vec![Binding::Select(Motion::WordLeft), Binding::Backspace])),
        Key::Named(Named::Delete) if ctrl => Some(Binding::Sequence(vec![Binding::Select(Motion::WordRight), Binding::Delete])),
        Key::Named(Named::Backspace) => Some(Binding::Custom(Msg::Backspace(id))),
        Key::Named(Named::Delete) => Some(Binding::Custom(Msg::Delete(id))),
        Key::Named(Named::ArrowUp) if !m.shift() && !ctrl => Some(Binding::Custom(Msg::Up(id))),
        Key::Named(Named::ArrowDown) if !m.shift() && !ctrl => Some(Binding::Custom(Msg::Down(id))),
        Key::Named(Named::ArrowLeft) if !m.shift() && !ctrl => Some(Binding::Custom(Msg::Left(id))),
        Key::Named(Named::ArrowRight) if !m.shift() && !ctrl => Some(Binding::Custom(Msg::Right(id))),
        Key::Named(Named::Tab) if multiline => Some(Binding::Sequence(vec![Binding::Insert(' '); 4])),
        // Other Ctrl combos (Ctrl+1, Ctrl+S, …) belong to the app, not the text.
        Key::Character(_) if ctrl && !m.alt() => None,
        _ => Binding::from_key_press(kp),
    }
}

/// Byte offset of a (line, column) position in the block's full text.
fn byte_offset(content: &Content, pos: Position) -> usize {
    let mut off = 0;
    for (i, line) in content.lines().enumerate() {
        if i == pos.line {
            return off + pos.column.min(line.text.len());
        }
        off += line.text.len() + line.ending.as_str().len();
    }
    off.saturating_sub(1)
}

/// Bold / italic / code / formula marks `m` like in a word processor: around the selection
/// (already wrapped: removed); at the cursor a pair to type into, or an empty pair right at
/// the cursor taken away again.
pub fn wrap_in(content: &mut Content, m: &str) {
    let cursor = content.cursor();
    match content.selection().filter(|s| !s.is_empty()) {
        Some(sel) => {
            let inner = sel.strip_prefix(m).and_then(|x| x.strip_suffix(m)).filter(|_| sel.len() >= 2 * m.len());
            let new = inner.map(str::to_string).unwrap_or_else(|| format!("{m}{sel}{m}"));
            content.perform(Action::Edit(Edit::Paste(std::sync::Arc::new(new))));
        }
        None => {
            let mut text = content.text();
            if text.ends_with('\n') {
                text.pop();
            }
            let off = byte_offset(content, cursor.position).min(text.len());
            if text[..off].ends_with(m) && text[off..].starts_with(m) {
                for _ in 0..m.len() {
                    content.perform(Action::Edit(Edit::Backspace));
                    content.perform(Action::Edit(Edit::Delete));
                }
            } else {
                content.perform(Action::Edit(Edit::Paste(std::sync::Arc::new(format!("{m}{m}")))));
                for _ in 0..m.len() {
                    content.perform(Action::Move(Motion::Left));
                }
            }
        }
    }
}

/// "12. rest" → "rest".
fn numbered(t: &str) -> Option<&str> {
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    t[digits..].strip_prefix(". ")
}

fn end_position(text: &str) -> Position {
    let line = text.matches('\n').count();
    let column = text.rsplit('\n').next().map(str::len).unwrap_or(0);
    Position { line, column }
}

/// Clamps a cursor to the content and to char boundaries (columns are byte offsets).
fn clamp_cursor(content: &Content, c: Cursor) -> Cursor {
    let fix = |p: Position| {
        let line = p.line.min(content.line_count().saturating_sub(1));
        let text = content.line(line).map(|l| l.text.to_string()).unwrap_or_default();
        let mut column = p.column.min(text.len());
        while !text.is_char_boundary(column) {
            column -= 1;
        }
        Position { line, column }
    };
    Cursor { position: fix(c.position), selection: c.selection.map(fix) }
}

/// A text block drawn with its formulas (and bold / italic / code): words in a wrapping row,
/// formulas as images. A click puts the cursor at its end.
fn rendered_text<'a>(b: &'a Block, font: Font, size: f32) -> Element<'a, Msg> {
    let mut lines = column![].spacing(4);
    for line in b.text().split('\n') {
        lines = lines.push(row(crate::markdown::inline_flow(line, size, font, theme::INK)).spacing(size * 0.28).align_y(Alignment::Center).wrap());
    }
    mouse_area(container(lines).padding(Padding::from([2, 0])).width(Length::Fill))
        .on_press(Msg::FocusEnd(b.id))
        .interaction(iced::mouse::Interaction::Text)
        .into()
}

/// A LaTeX formula (rendered by the local TeX; its source while that runs or if it fails).
pub fn formula_view<'a, M: 'a>(latex: &str, display: bool, size: f32) -> Element<'a, M> {
    match crate::latex::formula(latex, display) {
        crate::latex::Formula::Ready { handle, w, h } => {
            // TeX's 10 pt glyphs run small next to Space Grotesk at the same size.
            let k = size / 10.0 * 1.2;
            iced::widget::svg(handle).width(w * k).height(h * k).into()
        }
        crate::latex::Formula::Pending => text(format!("${latex}$")).font(theme::MONO).size(size * 0.85).color(theme::PAPER_MUTED).into(),
        crate::latex::Formula::Failed(e) => text(format!("${latex}$  ⚠ {e}")).font(theme::MONO).size(size * 0.8).color(theme::p().accent).into(),
    }
}

/// A small red square: the bullet glyph (no font dependency).
fn bullet<'a>() -> Element<'a, Msg> {
    container(Space::new().width(6).height(6))
        .style(|_| container::Style { background: Some(theme::p().accent.into()), ..Default::default() })
        .into()
}

#[cfg(test)]
mod markdown_tests {
    use super::*;

    #[test]
    fn lists_checklists_and_quotes_round_trip() {
        let md = "1. eins\n2. zwei\n\n- [ ] offen\n- [x] erledigt\n\n> Zitat\n\nText mit $x^2$";
        let doc = Doc::from_markdown(md);
        let kinds: Vec<Kind> = doc.blocks.iter().map(|b| b.kind.clone()).collect();
        assert_eq!(kinds, vec![Kind::Numbered, Kind::Numbered, Kind::Todo(false), Kind::Todo(true), Kind::Quote, Kind::Paragraph]);
        assert_eq!(doc.to_markdown(), md);
    }

    #[test]
    fn wrap_works_like_a_word_processor() {
        let sel = |c: &mut Content, a: usize, b: usize| c.move_to(Cursor { position: Position { line: 0, column: b }, selection: Some(Position { line: 0, column: a }) });
        let mut c = Content::with_text("hello world");
        sel(&mut c, 0, 5);
        wrap_in(&mut c, "**");
        assert_eq!(c.text().trim_end(), "**hello** world");
        // The wrapped word selected again: B takes it off.
        sel(&mut c, 0, 9);
        wrap_in(&mut c, "**");
        assert_eq!(c.text().trim_end(), "hello world");
        // At the cursor: a pair to type into; B again right away: gone.
        let mut c = Content::with_text("ab");
        c.move_to(Cursor { position: Position { line: 0, column: 1 }, selection: None });
        wrap_in(&mut c, "*");
        assert_eq!(c.text().trim_end(), "a**b");
        assert_eq!(c.cursor().position.column, 2);
        wrap_in(&mut c, "*");
        assert_eq!(c.text().trim_end(), "ab");
    }

    #[test]
    fn toolbar_sets_and_toggles_the_kind() {
        let mut doc = Doc::from_markdown("Absatz");
        let id = doc.blocks[0].id;
        doc.focus = Some(id);
        let _ = doc.update(Msg::SetKind(Kind::Heading(2)));
        assert_eq!(doc.blocks[0].kind, Kind::Heading(2));
        let _ = doc.update(Msg::SetKind(Kind::Todo(false)));
        let _ = doc.update(Msg::ToggleTodo(id));
        assert_eq!(doc.to_markdown(), "- [x] Absatz");
        let _ = doc.update(Msg::SetKind(Kind::Todo(false)));
        assert_eq!(doc.blocks[0].kind, Kind::Paragraph, "the same list kind again: back to a paragraph");
    }

    #[test]
    fn markdown_round_trips_through_blocks() {
        let md = "# Mitschrift V08\n\nErster Absatz\nmit Zeilenumbruch\n\n- eins\n- zwei\n\n```rust\nfn main() {}\n```\n\n$$\n\\int_0^1 x\\,dx\n$$\n\n## Ende";
        let doc = Doc::from_markdown(md);
        let kinds: Vec<Kind> = doc.blocks.iter().map(|b| b.kind.clone()).collect();
        assert_eq!(kinds, vec![Kind::Heading(1), Kind::Paragraph, Kind::Bullet, Kind::Bullet, Kind::Code { lang: "rust".into() }, Kind::Formula, Kind::Heading(2)]);
        assert_eq!(doc.blocks[1].text(), "Erster Absatz\nmit Zeilenumbruch");
        assert_eq!(doc.to_markdown(), md);
    }

    #[test]
    fn empty_markdown_is_one_empty_paragraph() {
        let doc = Doc::from_markdown("");
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.to_markdown(), "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(items: &[(&str, &str)]) -> Doc {
        Doc::new(items)
    }
    fn texts(d: &Doc) -> Vec<(Kind, String)> {
        d.blocks.iter().map(|b| (b.kind.clone(), b.text())).collect()
    }
    fn put(d: &mut Doc, i: usize, line: usize, column: usize) -> u64 {
        let id = d.blocks[i].id;
        d.blocks[i].content.move_to(Cursor { position: Position { line, column }, selection: None });
        d.focus = Some(id);
        id
    }
    fn typ(d: &mut Doc, id: u64, s: &str) {
        for c in s.chars() {
            let _ = d.update(Msg::Action(id, Action::Edit(Edit::Insert(c))));
        }
    }
    fn cursor_of(d: &Doc) -> (u64, Position) {
        let id = d.focus.unwrap();
        (id, d.blocks[d.index(id).unwrap()].content.cursor().position)
    }

    #[test]
    fn enter_splits_at_cursor_and_backspace_merges_back() {
        let mut d = doc(&[("", "Grüße aus Köln")]);
        let id = put(&mut d, 0, 0, "Grüße".len());
        let _ = d.update(Msg::Enter(id));
        assert_eq!(texts(&d), vec![(Kind::Paragraph, "Grüße".into()), (Kind::Paragraph, " aus Köln".into())]);
        let (nid, pos) = cursor_of(&d);
        assert_eq!(pos, Position { line: 0, column: 0 });
        assert_ne!(nid, id);
        let _ = d.update(Msg::Backspace(nid));
        assert_eq!(texts(&d), vec![(Kind::Paragraph, "Grüße aus Köln".into())]);
        assert_eq!(cursor_of(&d), (id, Position { line: 0, column: "Grüße".len() }));
    }

    #[test]
    fn markdown_shortcuts_convert_paragraphs() {
        let mut d = doc(&[("", "")]);
        let id = put(&mut d, 0, 0, 0);
        typ(&mut d, id, "# Titel");
        assert_eq!(texts(&d), vec![(Kind::Heading(1), "Titel".into())]);
        let _ = d.update(Msg::Enter(id));
        let (pid, _) = cursor_of(&d);
        typ(&mut d, pid, "- Punkt");
        assert_eq!(d.blocks[1].kind, Kind::Bullet);
        assert_eq!(d.blocks[1].text(), "Punkt");
        // Enter on a bullet continues the list; Enter on the empty bullet leaves it.
        let _ = d.update(Msg::Enter(pid));
        let (bid, _) = cursor_of(&d);
        assert_eq!(d.blocks[2].kind, Kind::Bullet);
        let _ = d.update(Msg::Enter(bid));
        assert_eq!(d.blocks[2].kind, Kind::Paragraph);
        typ(&mut d, bid, "```rust");
        let _ = d.update(Msg::Enter(bid));
        assert_eq!(d.blocks[2].kind, Kind::Code { lang: "rust".into() });
        assert_eq!(d.blocks[2].text(), "");
        let _ = d.update(Msg::ExitBlock(bid));
        let (fid, _) = cursor_of(&d);
        typ(&mut d, fid, "$$");
        assert_eq!(d.blocks[3].kind, Kind::Formula);
    }

    #[test]
    fn backspace_at_start_of_heading_demotes_first() {
        let mut d = doc(&[("", "a"), ("##", "Kopf")]);
        let id = put(&mut d, 1, 0, 0);
        let _ = d.update(Msg::Backspace(id));
        assert_eq!(texts(&d), vec![(Kind::Paragraph, "a".into()), (Kind::Paragraph, "Kopf".into())]);
        let _ = d.update(Msg::Backspace(id));
        assert_eq!(texts(&d), vec![(Kind::Paragraph, "aKopf".into())]);
    }

    #[test]
    fn delete_at_end_pulls_next_block_in() {
        let mut d = doc(&[("", "eins"), ("-", "zwei")]);
        let id = put(&mut d, 0, 0, 4);
        let _ = d.update(Msg::Delete(id));
        assert_eq!(texts(&d), vec![(Kind::Paragraph, "einszwei".into())]);
    }

    #[test]
    fn arrows_cross_block_edges() {
        let mut d = doc(&[("", "oben"), ("", "mitte\nzweite"), ("", "unten")]);
        let id = put(&mut d, 1, 0, 2);
        let _ = d.update(Msg::Up(id));
        assert_eq!(cursor_of(&d), (d.blocks[0].id, Position { line: 0, column: 2 }));
        let top = d.blocks[0].id;
        let _ = d.update(Msg::Down(top));
        assert_eq!(cursor_of(&d).0, id);
        let _ = d.update(Msg::Down(id)); // inside the block: to line 1
        assert_eq!(cursor_of(&d).0, id);
        let _ = d.update(Msg::Down(id)); // last line: next block
        assert_eq!(cursor_of(&d).0, d.blocks[2].id);
        let bottom = put(&mut d, 2, 0, 0);
        let _ = d.update(Msg::Left(bottom));
        assert_eq!(cursor_of(&d), (id, Position { line: 1, column: 6 }));
    }

    #[test]
    fn undo_and_redo_restore_structure_and_text() {
        let mut d = doc(&[("", "Hallo")]);
        let id = put(&mut d, 0, 0, 5);
        typ(&mut d, id, " Welt");
        let _ = d.update(Msg::Enter(id));
        assert_eq!(d.blocks.len(), 2);
        let _ = d.update(Msg::Undo);
        assert_eq!(texts(&d), vec![(Kind::Paragraph, "Hallo Welt".into())]);
        let _ = d.update(Msg::Undo);
        assert_eq!(texts(&d), vec![(Kind::Paragraph, "Hallo".into())]);
        let _ = d.update(Msg::Redo);
        assert_eq!(texts(&d), vec![(Kind::Paragraph, "Hallo Welt".into())]);
        let _ = d.update(Msg::Redo);
        assert_eq!(d.blocks.len(), 2);
    }

    #[test]
    fn umlauts_and_multibyte_columns_are_safe() {
        let mut d = doc(&[("", "")]);
        let id = put(&mut d, 0, 0, 0);
        typ(&mut d, id, "Äpfel, Öl, Übung, Straße – ξ ∈ ℝ");
        assert_eq!(d.blocks[0].text(), "Äpfel, Öl, Übung, Straße – ξ ∈ ℝ");
        // Split right after "Straße".
        let col = "Äpfel, Öl, Übung, Straße".len();
        put(&mut d, 0, 0, col);
        let _ = d.update(Msg::Enter(id));
        assert_eq!(d.blocks[0].text(), "Äpfel, Öl, Übung, Straße");
        assert_eq!(d.blocks[1].text(), " – ξ ∈ ℝ");
        // A cursor column inside a multibyte char is clamped, not a panic.
        let c = clamp_cursor(&d.blocks[0].content, Cursor { position: Position { line: 0, column: 1 }, selection: None });
        assert_eq!(c.position.column, 0);
    }
}
