//! The application: top bar, outline sidebar, page canvas or flowing text page, status bar.

use std::time::Instant;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding, Subscription, Task, Theme};

use n0te_core::notebook::{Notebook, OutlineRow};
use n0te_core::sheet::Sheet;

use crate::blocks::{self, Doc};
use crate::pages::{self, Camera, Pages};
use crate::{demo, sidebar, theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pages,
    Text,
}

#[derive(Debug, Clone)]
pub enum Message {
    Camera(Camera),
    ToggleGroup(String),
    Jump(String),
    SetMode(Mode),
    Zoom(f32),
    Blocks(blocks::Msg),
    Key(keyboard::Event),
}

pub struct App {
    pub notebook: Notebook,
    pub sheets: Vec<(String, Sheet)>,
    pub rows: Vec<OutlineRow>,
    pub pages: Pages,
    pub camera: Camera,
    pub mode: Mode,
    pub doc: Doc,
    /// Time to build the demo notebook (Loro docs) and to decode it into pages.
    pub build_ms: f32,
    pub decode_ms: f32,
    pub dense_sheet: String,
}

impl App {
    pub fn new() -> Self {
        let t = Instant::now();
        let demo = demo::notebook();
        let build_ms = t.elapsed().as_secs_f32() * 1000.0;
        let t = Instant::now();
        let pages = Pages::new(&demo.notebook, |id| demo.sheets.iter().find(|(s, _)| s == id).map(|(_, s)| s));
        let decode_ms = t.elapsed().as_secs_f32() * 1000.0;
        let rows = demo.notebook.outline(false);
        App {
            notebook: demo.notebook,
            sheets: demo.sheets,
            rows,
            pages,
            camera: Camera::default(),
            mode: Mode::Pages,
            doc: Doc::new(&demo::text_page()),
            build_ms,
            decode_ms,
            dense_sheet: demo.dense_sheet,
        }
    }

    pub fn boot() -> (Self, Task<Message>) {
        (Self::new(), Task::none())
    }

    pub fn theme(&self) -> Theme {
        theme::theme()
    }

    pub fn title(&self) -> String {
        format!("n0te — {}", self.notebook.title())
    }

    pub fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().map(Message::Key)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Camera(c) => self.camera = c,
            Message::ToggleGroup(node) => {
                let collapsed = self.rows.iter().any(|r| matches!(r, OutlineRow::Group { node: n, collapsed: true, .. } if *n == node));
                if self.notebook.set_collapsed(&node, !collapsed).is_ok() {
                    self.rows = self.notebook.outline(false);
                }
            }
            Message::Jump(sheet) => {
                self.mode = Mode::Pages;
                if let Some(i) = self.pages.page_of_sheet(&sheet) {
                    self.camera = self.pages.camera_for(i, &self.camera);
                }
            }
            Message::SetMode(m) => self.mode = m,
            Message::Zoom(z) => {
                let c = self.camera;
                // Zoom around the viewport centre.
                let (cx, cy) = (c.x + c.viewport.width / c.zoom / 2.0, c.y + c.viewport.height / c.zoom / 2.0);
                let zoom = z.clamp(pages::MIN_ZOOM, pages::MAX_ZOOM);
                self.camera = self.pages.clamp(Camera { x: cx - c.viewport.width / zoom / 2.0, y: cy - c.viewport.height / zoom / 2.0, zoom, ..c });
            }
            Message::Blocks(m) => return self.doc.update(m).map(Message::Blocks),
            Message::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => return self.shortcut(key, modifiers),
            Message::Key(_) => {}
        }
        Task::none()
    }

    /// Global shortcuts (only keys no focused widget consumed).
    fn shortcut(&mut self, key: Key, m: keyboard::Modifiers) -> Task<Message> {
        let c = self.camera;
        if m.command() {
            return match key.as_ref() {
                Key::Character("1") => self.update(Message::SetMode(Mode::Pages)),
                Key::Character("2") => self.update(Message::SetMode(Mode::Text)),
                Key::Character("0") => self.update(Message::Zoom(1.0)),
                Key::Character("+" | "=") => self.update(Message::Zoom(c.zoom * 1.25)),
                Key::Character("-") => self.update(Message::Zoom(c.zoom / 1.25)),
                _ => Task::none(),
            };
        }
        if self.mode == Mode::Pages {
            let current = self.pages.current_number(&c);
            let target = match key.as_ref() {
                Key::Named(Named::PageDown) => Some(current + 1),
                Key::Named(Named::PageUp) => Some(current.saturating_sub(1).max(1)),
                Key::Named(Named::Home) => Some(1),
                Key::Named(Named::End) => Some(self.pages.mains.len()),
                _ => None,
            };
            if let Some(n) = target
                && let Some(&i) = self.pages.mains.get(n.saturating_sub(1))
            {
                self.camera = self.pages.camera_for(i, &c);
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let current = self.pages.current_number(&self.camera);
        let content: Element<'_, Message> = match self.mode {
            Mode::Pages => pages::page_view(&self.pages, self.camera, Message::Camera).into(),
            Mode::Text => self.text_page(),
        };
        let body = row![
            sidebar::view(&self.rows, &self.pages, self.mode, current),
            container(Space::new()).width(1).height(Length::Fill).style(theme::hairline),
            content
        ];
        column![
            self.top_bar(current),
            container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
            body.height(Length::Fill),
            container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
            self.status_bar()
        ]
        .into()
    }

    fn top_bar(&self, current: usize) -> Element<'_, Message> {
        let tab = |label: &'static str, mode: Mode| {
            button(text(label).font(theme::MONO_BOLD).size(theme::LABEL))
                .padding([5, 14])
                .on_press(Message::SetMode(mode))
                .style(theme::tab(self.mode == mode))
        };
        let wordmark = row![
            text("n0te").font(theme::DOT).size(30).color(theme::TEXT),
            container(Space::new().width(7).height(7)).style(|_| container::Style { background: Some(theme::ACCENT.into()), border: iced::Border { radius: 4.0.into(), ..Default::default() }, ..Default::default() }),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let title = self.notebook.title().to_uppercase();
        let right: Element<'_, Message> = match self.mode {
            Mode::Pages => row![
                container(text(format!("{:>3.0} %", self.camera.zoom * 100.0)).font(theme::MONO).size(theme::LABEL)).padding([3, 10]).style(theme::chip),
                container(text(format!("{current:02} / {:02}", self.pages.mains.len())).font(theme::MONO).size(theme::LABEL)).padding([3, 10]).style(theme::chip),
            ]
            .spacing(8)
            .into(),
            Mode::Text => container(text(format!("{} BLÖCKE", self.doc.blocks.len())).font(theme::MONO).size(theme::LABEL)).padding([3, 10]).style(theme::chip).into(),
        };
        container(
            row![
                wordmark,
                Space::new().width(22),
                text(title).font(theme::MONO).size(theme::SMALL).color(theme::TEXT),
                Space::new().width(Length::Fill),
                row![tab("SEITEN", Mode::Pages), tab("TEXT", Mode::Text)].spacing(6),
                Space::new().width(Length::Fill),
                right,
            ]
            .align_y(Alignment::Center),
        )
        .padding(Padding { top: 6.0, bottom: 6.0, left: 18.0, right: 14.0 })
        .height(52)
        .width(Length::Fill)
        .style(theme::chrome)
        .into()
    }

    fn status_bar(&self) -> Element<'_, Message> {
        let s = &self.pages.stats;
        let left = format!(
            "{} SEITEN · {} STRICHE · SICHTBAR {} / {} STR · CACHE {} · BUILD {:.1} MS (MAX {:.1}) · DRAW {:.2} MS",
            self.pages.pages.len(),
            self.pages.stroke_count(),
            s.visible_pages.get(),
            s.visible_strokes.get(),
            self.pages.cached_pages(),
            s.last_build_ms.get(),
            s.max_build_ms.get(),
            s.last_draw_ms.get(),
        );
        container(
            row![
                text(left).font(theme::MONO).size(10.5).color(theme::MUTED),
                Space::new().width(Length::Fill),
                text("ICED 0.14 · SPIKE").font(theme::MONO).size(10.5).color(theme::MUTED),
            ]
            .align_y(Alignment::Center),
        )
        .padding([4, 14])
        .height(26)
        .width(Length::Fill)
        .style(theme::chrome)
        .into()
    }

    fn text_page(&self) -> Element<'_, Message> {
        let header = row![
            text("MITSCHRIFT · TEXTSEITE").font(theme::MONO).size(10.5).color(theme::PAPER_MUTED),
            Space::new().width(Length::Fill),
            text(format!("{} BLÖCKE", self.doc.blocks.len())).font(theme::MONO).size(10.5).color(theme::PAPER_MUTED),
        ];
        let paper = container(column![header, Space::new().height(18), self.doc.view().map(Message::Blocks)])
            .padding(Padding { top: 36.0, right: 64.0, bottom: 72.0, left: 36.0 })
            .max_width(800)
            .width(Length::Fill)
            .style(theme::paper);
        let page = container(paper).padding(Padding { top: 32.0, right: 32.0, bottom: 48.0, left: 32.0 }).center_x(Length::Fill);
        container(scrollable(page).style(theme::scroll_dark).height(Length::Fill).width(Length::Fill))
            .style(theme::surface)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
