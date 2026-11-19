//! The library screen: notebooks grouped by folder as cards with their cover, `.n0te` bundles
//! waiting in Downloads (from the tablet), a new notebook.

use std::time::SystemTime;

use iced::widget::canvas::{self, Path, Stroke};
use iced::widget::{Space, button, canvas as canvas_widget, column, container, row, text};
use iced::{Alignment, Color, Element, Length, Padding, Point, Rectangle, Renderer, Size, Theme, mouse};

use std::collections::HashSet;
use std::path::PathBuf;

use crate::app::{App, Bundle, Entry, ExportWhat, Message, Open};
use crate::theme;

pub fn view(app: &App) -> Element<'_, Message> {
    let imports = if app.bundles.is_empty() { "IMPORTIEREN".to_string() } else { format!("IMPORTIEREN · {}", app.bundles.len()) };
    let top = container(
        row![
            wordmark(),
            Space::new().width(18),
            text("BIBLIOTHEK").font(theme::MONO).size(theme::SMALL).color(theme::p().muted),
            Space::new().width(Length::Fill),
            button(text("EINLADUNG").font(theme::MONO_BOLD).size(theme::LABEL)).padding([6, 12]).on_press(Message::StartJoin).style(theme::tab(app.joining.is_some())),
            Space::new().width(8),
            button(text("PDF IMPORTIEREN").font(theme::MONO_BOLD).size(theme::LABEL)).padding([6, 12]).on_press(Message::ShowPdfImport(!app.show_pdf_import)).style(theme::tab(app.show_pdf_import)),
            Space::new().width(8),
            button(text(imports).font(theme::MONO_BOLD).size(theme::LABEL)).padding([6, 12]).on_press(Message::ShowImports(!app.show_imports)).style(theme::tab(app.show_imports)),
            Space::new().width(8),
            button(text("+ NOTIZBUCH").font(theme::MONO_BOLD).size(theme::LABEL)).padding([6, 12]).on_press(Message::StartCreate).style(theme::accent_button),
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 8.0, bottom: 8.0, left: 18.0, right: 14.0 })
    .height(56)
    .width(Length::Fill)
    .style(theme::chrome);

    let mut content = column![].spacing(18).padding(Padding { top: 18.0, right: 24.0, bottom: 32.0, left: 24.0 });
    if let Some(r) = app.create_row() {
        content = content.push(r);
    }
    if let Some(link) = &app.joining {
        content = content.push(
            container(
                column![
                    text("EINLADUNG ÖFFNEN — DAS NOTIZBUCH WIRD DIREKT VOM BESITZER GELADEN").font(theme::MONO).size(theme::LABEL).color(theme::p().muted),
                    row![
                        iced::widget::text_input("n0te://invite/…", link).on_input(Message::JoinLink).on_submit(Message::Join).padding(10).size(14),
                        Space::new().width(8),
                        button(text("BEITRETEN").font(theme::MONO_BOLD).size(theme::LABEL)).padding([8, 14]).on_press(Message::Join).style(theme::accent_button),
                    ]
                    .align_y(Alignment::Center),
                ]
                .spacing(8),
            )
            .padding(14)
            .width(Length::Fill)
            .max_width(900)
            .style(theme::panel),
        );
    }
    if app.show_imports {
        content = content.push(imports_panel(&app.bundles));
    }
    if app.show_pdf_import {
        content = content.push(pdf_panel(&app.pdfs, "PDF IMPORTIEREN — WIRD EIN NEUES NOTIZBUCH"));
    }
    if app.entries.is_empty() {
        content = content.push(
            column![
                text("Noch keine Notizbücher auf dem Laptop.").size(18).color(theme::p().text),
                text("Auf dem Tablet: Notizbuch halten → AN DEN LAPTOP. Es kommt über n0bridge und erscheint unter IMPORTIEREN.").size(14).color(theme::p().muted),
            ]
            .spacing(8),
        );
    }
    let mut folder: Option<&str> = None;
    let mut cards = Vec::new();
    for e in &app.entries {
        if folder != Some(e.folder.as_str()) {
            content = flush(content, folder, &mut cards);
            folder = Some(e.folder.as_str());
        }
        cards.push(card(e));
    }
    content = flush(content, folder, &mut cards);

    column![
        top,
        container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
        container(App::scroll(content)).height(Length::Fill).style(theme::surface),
        container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
        app.status_bar(),
    ]
    .into()
}

/// Adds a folder's cards (wrapping grid) under its label.
fn flush<'a>(content: iced::widget::Column<'a, Message>, folder: Option<&str>, cards: &mut Vec<Element<'a, Message>>) -> iced::widget::Column<'a, Message> {
    if cards.is_empty() {
        return content;
    }
    let label = folder.filter(|f| !f.is_empty()).unwrap_or("NOTIZBÜCHER").to_uppercase();
    let grid = iced::widget::Row::with_children(std::mem::take(cards)).spacing(14).wrap().vertical_spacing(14);
    content.push(column![text(label).font(theme::MONO).size(theme::LABEL).color(theme::p().muted), grid].spacing(8))
}

/// "n0te" in the dot face with the red dot.
pub fn wordmark<'a>() -> Element<'a, Message> {
    row![
        text("n0te").font(theme::DOT).size(30).color(theme::p().text),
        container(Space::new().width(7).height(7)).style(|_| container::Style { background: Some(theme::p().accent.into()), border: iced::Border { radius: 4.0.into(), ..Default::default() }, ..Default::default() }),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn card(e: &Entry) -> Element<'_, Message> {
    let pages = format!("{} {}", e.sheets, if e.sheets == 1 { "Seite" } else { "Seiten" });
    button(
        column![
            canvas_widget(Cover { kind: e.cover.clone(), title: e.title.clone() }).width(196).height(132),
            Space::new().height(8),
            text(fit(&e.title, 24)).font(theme::SANS_MEDIUM).size(15).color(theme::p().text),
            text(pages).font(theme::MONO).size(theme::LABEL).color(theme::p().muted),
        ]
        .spacing(2),
    )
    .padding(10)
    .on_press(Message::Open(e.id.clone()))
    .style(theme::card)
    .into()
}

fn imports_panel(bundles: &[Bundle]) -> Element<'_, Message> {
    let mut list = column![text("VOM TABLET (~/Downloads/n0bridge)").font(theme::MONO).size(theme::LABEL).color(theme::p().muted)].spacing(6);
    if bundles.is_empty() {
        list = list.push(text("Keine .n0te-Dateien gefunden. Auf dem Tablet: Notizbuch halten → AN DEN LAPTOP.").size(13).color(theme::p().muted));
    }
    for b in bundles {
        let name = b.path.file_stem().map(|s| s.to_string_lossy().replace('_', " ")).unwrap_or_default();
        list = list.push(
            row![
                text(name).size(14).color(theme::p().text),
                Space::new().width(12),
                text(age(b.modified)).font(theme::MONO).size(theme::LABEL).color(theme::p().muted),
                Space::new().width(Length::Fill),
                button(text("ÜBERNEHMEN").font(theme::MONO_BOLD).size(theme::LABEL)).padding([5, 12]).on_press(Message::Import(b.path.clone())).style(theme::tab(false)),
            ]
            .align_y(Alignment::Center),
        );
    }
    list = list.push(text("Übernehmen führt mit dem Stand auf dem Laptop zusammen: nichts geht verloren.").size(12).color(theme::p().muted));
    container(list).padding(14).width(Length::Fill).style(theme::panel).into()
}

/// Recent PDFs to import; dropping a file onto the window works too.
pub fn pdf_panel<'a>(pdfs: &'a [(PathBuf, SystemTime)], title: &'a str) -> Element<'a, Message> {
    let mut list = column![text(title).font(theme::MONO).size(theme::LABEL).color(theme::p().muted)].spacing(6);
    if pdfs.is_empty() {
        list = list.push(text("Keine PDFs in Downloads oder Dokumente.").size(13).color(theme::p().muted));
    }
    for (path, modified) in pdfs.iter().take(14) {
        let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let dir = path.parent().and_then(|d| d.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        list = list.push(
            row![
                text(fit(&name, 60)).size(14).color(theme::p().text),
                Space::new().width(10),
                text(format!("{dir} · {}", age(*modified))).font(theme::MONO).size(theme::LABEL).color(theme::p().muted),
                Space::new().width(Length::Fill),
                button(text("NEHMEN").font(theme::MONO_BOLD).size(theme::LABEL)).padding([5, 12]).on_press(Message::ImportPdf(path.clone())).style(theme::tab(false)),
            ]
            .align_y(Alignment::Center),
        );
    }
    list = list.push(text("Oder eine PDF- bzw. .n0te-Datei ins Fenster ziehen.").size(12).color(theme::p().muted));
    container(list).padding(14).width(Length::Fill).max_width(900).style(theme::panel).into()
}

/// Export: whole notebook, this page, or the picked pages (with their side pages).
pub fn export_panel<'a>(o: &'a Open, picked: &'a HashSet<String>) -> Element<'a, Message> {
    let mut pages = column![].spacing(2);
    for p in o.pages.pages.iter().filter(|p| p.placed.side == 0) {
        let on = picked.contains(&p.placed.node);
        let sides = o.pages.pages.iter().filter(|s| s.placed.main_node == p.placed.node && s.placed.side != 0).count();
        let label = format!("{}  {:02}{}", if on { "■" } else { "□" }, p.placed.number, if sides > 0 { format!("  +{sides}") } else { String::new() });
        pages = pages.push(button(text(label).font(theme::MONO).size(12)).padding([3, 8]).on_press(Message::TogglePick(p.placed.node.clone())).style(theme::row(on)));
    }
    let list = iced::widget::scrollable(pages).style(theme::scroll_dark).height(Length::Fixed(260.0));
    container(
        column![
            text("PDF EXPORTIEREN — NACH ~/Downloads").font(theme::MONO).size(theme::LABEL).color(theme::p().muted),
            row![
                button(text("GANZES NOTIZBUCH").font(theme::MONO_BOLD).size(theme::LABEL)).padding([6, 12]).on_press(Message::Export(ExportWhat::All)).style(theme::accent_button),
                Space::new().width(8),
                button(text("DIESE SEITE").font(theme::MONO_BOLD).size(theme::LABEL)).padding([6, 12]).on_press(Message::Export(ExportWhat::Current)).style(theme::tab(false)),
                Space::new().width(8),
                button(text(format!("AUSWAHL ({})", picked.len())).font(theme::MONO_BOLD).size(theme::LABEL)).padding([6, 12]).on_press(Message::Export(ExportWhat::Picked)).style(theme::tab(false)),
                Space::new().width(Length::Fill),
                button(text("SCHLIESSEN").font(theme::MONO_BOLD).size(theme::LABEL)).padding([6, 12]).on_press(Message::ShowExport(false)).style(theme::ghost),
            ]
            .align_y(Alignment::Center),
            text("Für AUSWAHL Seiten anklicken (Seitenblätter kommen mit):").size(12).color(theme::p().muted),
            list,
        ]
        .spacing(10),
    )
    .padding(14)
    .width(Length::Fill)
    .max_width(640)
    .style(theme::panel)
    .into()
}

fn age(t: SystemTime) -> String {
    let s = SystemTime::now().duration_since(t).map(|d| d.as_secs()).unwrap_or(0);
    match s {
        0..=59 => "gerade eben".into(),
        60..=3599 => format!("vor {} min", s / 60),
        3600..=86_399 => format!("vor {} h", s / 3600),
        _ => format!("vor {} Tagen", s / 86_400),
    }
}

/// iced text has no ellipsis: cut long titles.
pub fn fit(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max - 1).collect::<String>().trim_end()) }
}

/// A notebook cover, like on the tablet: first page (plain paper here for now), the initial in
/// dots, a dot grid, stripes, or a colour.
struct Cover {
    kind: String,
    title: String,
}

fn colour(kind: &str) -> Option<Color> {
    Some(match kind {
        "black" => iced::color!(0x161618),
        "red" => iced::color!(0xD71921),
        "blue" => iced::color!(0x1E5BD8),
        "green" => iced::color!(0x138A47),
        "orange" => iced::color!(0xE08A00),
        "purple" => iced::color!(0x7B3FD0),
        _ => return None,
    })
}

impl<M> canvas::Program<M> for Cover {
    type State = ();

    fn draw(&self, _state: &(), renderer: &Renderer, _theme: &Theme, bounds: Rectangle, _cursor: mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut f = canvas::Frame::new(renderer, bounds.size());
        let (w, h) = (bounds.width, bounds.height);
        let dark = iced::color!(0x111113);
        let initial = self.title.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "N".into());
        let big = |f: &mut canvas::Frame, color: Color, size: f32| {
            f.fill_text(canvas::Text {
                content: initial.clone(),
                position: Point::new(w / 2.0, h / 2.0),
                color,
                size: size.into(),
                font: theme::DOT,
                align_x: iced::widget::text::Alignment::Center,
                align_y: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            });
        };
        match self.kind.as_str() {
            "dots" => {
                f.fill_rectangle(Point::ORIGIN, Size::new(w, h), dark);
                let dots = Path::new(|b| {
                    let mut y = 7.0;
                    while y < h {
                        let mut x = 7.0;
                        while x < w {
                            b.circle(Point::new(x, y), 1.4);
                            x += 14.0;
                        }
                        y += 14.0;
                    }
                });
                f.fill(&dots, iced::color!(0x3A3A40));
                f.fill(&Path::circle(Point::new(w - 21.0, 21.0), 5.0), theme::p().accent);
            }
            "initial" => {
                f.fill_rectangle(Point::ORIGIN, Size::new(w, h), dark);
                big(&mut f, iced::color!(0xF2F2F4), h * 0.62);
                f.fill(&Path::circle(Point::new(w * 0.82, h * 0.2), 4.0), theme::p().accent);
            }
            "stripes" => {
                f.fill_rectangle(Point::ORIGIN, Size::new(w, h), dark);
                let mut x = -h;
                while x < w {
                    f.stroke(&Path::line(Point::new(x, h), Point::new(x + h, 0.0)), Stroke::default().with_color(theme::p().accent).with_width(5.0));
                    x += 22.0;
                }
            }
            k if colour(k).is_some() => {
                f.fill_rectangle(Point::ORIGIN, Size::new(w, h), colour(k).expect("checked"));
                big(&mut f, Color { a: 0.9, ..Color::WHITE }, h * 0.5);
            }
            _ => {
                f.fill_rectangle(Point::ORIGIN, Size::new(w, h), theme::p().paper);
                if !theme::p().dark {
                    f.stroke(&Path::rectangle(Point::new(0.5, 0.5), Size::new(w - 1.0, h - 1.0)), Stroke::default().with_color(theme::p().border).with_width(1.0));
                }
                let mut y = 24.0;
                for _ in 0..6 {
                    f.stroke(&Path::line(Point::new(14.0, y), Point::new(w - 14.0, y)), Stroke::default().with_color(iced::color!(0xE3E7EE)).with_width(1.0));
                    y += 17.0;
                }
            }
        }
        vec![f.into_geometry()]
    }
}
