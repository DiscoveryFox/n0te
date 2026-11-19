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
    let imports = if app.bundles.is_empty() { "Importieren".to_string() } else { format!("Importieren · {}", app.bundles.len()) };
    let top = container(
        row![
            wordmark(),
            Space::new().width(Length::Fill),
            sync_chip(app),
            Space::new().width(12),
            button(theme::btn("Einladung")).padding(theme::pad()).on_press(Message::StartJoin).style(theme::tab(app.joining.is_some())),
            Space::new().width(8),
            button(theme::btn(&imports)).padding(theme::pad()).on_press(Message::ShowImports(!app.show_imports)).style(theme::tab(app.show_imports)),
            Space::new().width(8),
            button(theme::btn("+ Neues Notizbuch")).padding(theme::pad()).on_press(Message::StartCreate).style(theme::accent_button),
            Space::new().width(8),
            button(theme::btn("Einstellungen")).padding(theme::pad()).on_press(Message::ShowSettings(true)).style(theme::ghost),
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 8.0, bottom: 8.0, left: 20.0, right: 16.0 })
    .height(58)
    .width(Length::Fill)
    .style(theme::chrome);

    let folders = app.entries.iter().map(|e| e.folder.as_str()).filter(|f| !f.is_empty()).collect::<HashSet<_>>().len();
    let n = app.entries.len();
    let mut facts = format!("{n} {}", if n == 1 { "Notizbuch" } else { "Notizbücher" });
    if folders > 0 {
        facts += &format!(" · {folders} {}", if folders == 1 { "Ordner" } else { "Ordner" });
    }
    let mut content = column![column![theme::heading("Bibliothek", 30.0), text(facts).font(theme::meta_font()).size(13).color(theme::p().muted)].spacing(2)]
        .spacing(26)
        .padding(Padding { top: 26.0, right: 32.0, bottom: 40.0, left: 32.0 });
    if let Some(r) = app.create_row() {
        content = content.push(r);
    }
    if let Some(link) = &app.joining {
        content = content.push(
            container(
                column![
                    theme::caption("Einladung öffnen"),
                    row![
                        iced::widget::text_input("n0te://invite/…", link).on_input(Message::JoinLink).on_submit(Message::Join).padding(10).size(14).style(theme::field),
                        Space::new().width(8),
                        button(theme::btn("Beitreten")).padding(theme::pad()).on_press(Message::Join).style(theme::accent_button),
                        Space::new().width(6),
                        button(theme::btn("Abbrechen")).padding(theme::pad()).on_press(Message::CancelJoin).style(theme::ghost),
                    ]
                    .align_y(Alignment::Center),
                    text("Das Notizbuch wird direkt vom Gerät geladen, das es geteilt hat, ohne Server.").size(12).color(theme::p().muted),
                ]
                .spacing(10),
            )
            .padding(18)
            .width(Length::Fill)
            .max_width(900)
            .style(theme::panel),
        );
    }
    if app.show_imports {
        content = content.push(row![imports_panel(&app.bundles), pdf_panel(&app.pdfs, "PDF importieren: wird ein neues Notizbuch")].spacing(16));
    } else if app.show_pdf_import {
        content = content.push(pdf_panel(&app.pdfs, "PDF importieren: wird ein neues Notizbuch"));
    }
    if app.entries.is_empty() {
        content = content.push(
            container(
                column![
                    theme::heading("Noch leer", 22.0),
                    text("Leg ein Notizbuch an, nimm eins vom Tablet (dort: Notizbuch halten → An den Laptop) oder importiere eine PDF.").size(14).color(theme::p().muted),
                    row![
                        button(theme::btn("+ Neues Notizbuch")).padding(theme::pad()).on_press(Message::StartCreate).style(theme::accent_button),
                        Space::new().width(8),
                        button(theme::btn("Importieren")).padding(theme::pad()).on_press(Message::ShowImports(true)).style(theme::secondary),
                    ],
                ]
                .spacing(12),
            )
            .padding(24)
            .max_width(620)
            .style(theme::group),
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

/// "● Tablet verbunden" (or not reachable) when a tablet is paired.
fn sync_chip(app: &App) -> Element<'_, Message> {
    if app.sync.is_none() || app.devices.is_empty() {
        return Space::new().into();
    }
    let d = app.devices.iter().find(|d| d.online).unwrap_or(&app.devices[0]);
    let label = if d.online { format!("{} verbunden", d.name) } else { format!("{} nicht erreichbar", d.name) };
    container(
        row![
            container(Space::new().width(7).height(7)).style(move |_| container::Style {
                background: Some((if d.online { theme::p().accent } else { theme::p().muted }).into()),
                border: iced::Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            }),
            text(theme::cased(&label)).font(theme::meta_font()).size(if theme::s().label == theme::Label::MonoCaps { theme::LABEL } else { 12.5 }).color(if d.online { theme::p().text } else { theme::p().muted }),
        ]
        .spacing(7)
        .align_y(Alignment::Center),
    )
    .padding([5, 11])
    .style(theme::chip)
    .into()
}

/// Adds a folder's cards (wrapping grid) under its heading and a hairline.
fn flush<'a>(content: iced::widget::Column<'a, Message>, folder: Option<&str>, cards: &mut Vec<Element<'a, Message>>) -> iced::widget::Column<'a, Message> {
    if cards.is_empty() {
        return content;
    }
    let label = folder.filter(|f| !f.is_empty()).unwrap_or("Notizbücher").to_string();
    let head = row![
        theme::caption(&label),
        Space::new().width(10),
        text(cards.len().to_string()).font(theme::meta_font()).size(11.5).color(theme::alpha(theme::p().muted, 0.8)),
        Space::new().width(14),
        container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
    ]
    .align_y(Alignment::Center);
    let grid = iced::widget::Row::with_children(std::mem::take(cards)).spacing(20).wrap().vertical_spacing(22);
    content.push(column![head, grid].spacing(14))
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

const COVER: (f32, f32) = (164.0, 212.0);

fn card(e: &Entry) -> Element<'_, Message> {
    let mut meta = format!("{} {}", e.sheets, if e.sheets == 1 { "Seite" } else { "Seiten" });
    if let Some(t) = e.modified {
        meta += &format!(" · {}", age(t));
    }
    button(
        column![
            canvas_widget(Cover { kind: e.cover.clone(), title: e.title.clone() }).width(COVER.0).height(COVER.1),
            Space::new().height(10),
            text(fit(&e.title, 21)).font(theme::title_font()).size(15).color(theme::p().text),
            text(theme::cased(&meta)).font(theme::meta_font()).size(if theme::s().label == theme::Label::MonoCaps { theme::LABEL } else { 12.0 }).color(theme::p().muted),
        ]
        .spacing(3),
    )
    .padding(10)
    .width(COVER.0 + 20.0)
    .on_press(Message::Open(e.id.clone()))
    .style(theme::card)
    .into()
}

fn imports_panel(bundles: &[Bundle]) -> Element<'_, Message> {
    let mut list = column![theme::caption("Vom Tablet (~/Downloads/n0bridge)")].spacing(8);
    if bundles.is_empty() {
        list = list.push(text("Keine .n0te-Dateien gefunden. Auf dem Tablet: Notizbuch halten → An den Laptop.").size(13).color(theme::p().muted));
    }
    for b in bundles {
        let name = b.path.file_stem().map(|s| s.to_string_lossy().replace('_', " ")).unwrap_or_default();
        list = list.push(
            row![
                column![text(name).size(14).color(theme::p().text), text(age(b.modified)).font(theme::meta_font()).size(11.5).color(theme::p().muted)].spacing(1),
                Space::new().width(Length::Fill),
                button(theme::btn_small("Übernehmen")).padding(theme::pad()).on_press(Message::Import(b.path.clone())).style(theme::secondary),
            ]
            .align_y(Alignment::Center),
        );
    }
    list = list.push(text("Übernehmen führt mit dem Stand auf dem Laptop zusammen: nichts geht verloren.").size(12).color(theme::p().muted));
    container(list).padding(18).width(Length::Fill).style(theme::panel).into()
}

/// Recent PDFs to import; dropping a file onto the window works too.
pub fn pdf_panel<'a>(pdfs: &'a [(PathBuf, SystemTime)], title: &'a str) -> Element<'a, Message> {
    let mut list = column![theme::caption(title)].spacing(8);
    if pdfs.is_empty() {
        list = list.push(text("Keine PDFs in Downloads oder Dokumente.").size(13).color(theme::p().muted));
    }
    for (path, modified) in pdfs.iter().take(12) {
        let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let dir = path.parent().and_then(|d| d.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        list = list.push(
            row![
                column![text(fit(&name, 52)).size(14).color(theme::p().text), text(format!("{dir} · {}", age(*modified))).font(theme::meta_font()).size(11.5).color(theme::p().muted)].spacing(1),
                Space::new().width(Length::Fill),
                button(theme::btn_small("Nehmen")).padding(theme::pad()).on_press(Message::ImportPdf(path.clone())).style(theme::secondary),
            ]
            .align_y(Alignment::Center),
        );
    }
    list = list.push(text("Oder eine PDF- bzw. .n0te-Datei ins Fenster ziehen.").size(12).color(theme::p().muted));
    container(list).padding(18).width(Length::Fill).max_width(900).style(theme::panel).into()
}

/// Export as a dialog: pick the scope (whole notebook, this page, a selection), then one
/// primary action. Selected pages take their side pages along.
pub fn export_dialog<'a>(o: &'a Open, picked: &'a HashSet<String>, mode: ExportWhat, current: u32) -> Element<'a, Message> {
    let tile = |what: ExportWhat, title: &str, sub: String| {
        button(column![text(title.to_string()).font(theme::title_font()).size(15).color(theme::p().text), text(sub).size(12).color(theme::p().muted)].spacing(4))
            .padding([14, 16])
            .width(Length::Fill)
            .on_press(Message::ExportMode(what))
            .style(theme::option(mode == what))
    };
    let mut col = column![
        row![theme::heading("PDF exportieren", 22.0), Space::new().width(Length::Fill), button(theme::btn_small("Schließen")).padding(theme::pad()).on_press(Message::ShowExport(false)).style(theme::ghost)].align_y(Alignment::Center),
        theme::caption("Umfang"),
        row![
            tile(ExportWhat::All, "Ganzes Notizbuch", format!("{} Seiten mit Seitenblättern", o.pages.total_pages)),
            tile(ExportWhat::Current, "Diese Seite", format!("Seite {current} und ihre Seitenblätter")),
            tile(ExportWhat::Picked, "Auswahl", if picked.is_empty() { "Seiten unten antippen".to_string() } else { format!("{} {} gewählt", picked.len(), if picked.len() == 1 { "Seite" } else { "Seiten" }) }),
        ]
        .spacing(10),
    ]
    .spacing(14);
    if mode == ExportWhat::Picked {
        let mut chips = Vec::new();
        for p in o.pages.pages.iter().filter(|p| p.placed.side == 0) {
            let on = picked.contains(&p.placed.node);
            let sides = o.pages.pages.iter().filter(|s| s.placed.main_node == p.placed.node && s.placed.side != 0).count();
            let label = format!("{:02}{}", p.placed.number, if sides > 0 { format!(" +{sides}") } else { String::new() });
            chips.push(button(text(label).font(theme::MONO).size(12)).padding([6, 11]).on_press(Message::TogglePick(p.placed.node.clone())).style(theme::tab(on)).into());
        }
        col = col.push(theme::caption("Seiten (+n: Seitenblätter, kommen mit)"));
        col = col.push(iced::widget::scrollable(iced::widget::Row::with_children(chips).spacing(6).wrap().vertical_spacing(6)).style(theme::scroll_dark).height(Length::Fixed(190.0)));
    }
    let ready = mode != ExportWhat::Picked || !picked.is_empty();
    col = col.push(container(Space::new()).height(1).width(Length::Fill).style(theme::hairline));
    col = col.push(
        row![
            text("Nach ~/Downloads · Seiten bleiben weiß, auch in dunklen Designs").font(theme::meta_font()).size(12).color(theme::p().muted),
            Space::new().width(Length::Fill),
            button(theme::btn("Abbrechen")).padding(theme::pad()).on_press(Message::ShowExport(false)).style(theme::ghost),
            Space::new().width(8),
            button(theme::btn("Exportieren")).padding(theme::pad()).on_press_maybe(ready.then_some(Message::Export(mode))).style(theme::accent_button),
        ]
        .align_y(Alignment::Center),
    );
    container(container(col).padding(24).width(Length::Fixed(700.0)).style(theme::panel)).padding(Padding { top: 80.0, ..Padding::ZERO }).center_x(Length::Fill).into()
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
                let mut y = 30.0;
                while y < h - 12.0 {
                    f.stroke(&Path::line(Point::new(14.0, y), Point::new(w - 14.0, y)), Stroke::default().with_color(iced::color!(0xE3E7EE)).with_width(1.0));
                    y += 17.0;
                }
            }
        }
        vec![f.into_geometry()]
    }
}
