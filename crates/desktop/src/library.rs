//! The library screen: a sidebar with folders and tags (see `shelf`), the notebooks of what is
//! picked there as cards with their cover, `.n0te` bundles waiting in Downloads (from the
//! tablet), a new notebook.

use std::time::SystemTime;

use iced::widget::canvas::{self, Path, Stroke};
use iced::widget::{Space, button, canvas as canvas_widget, column, container, hover, mouse_area, row, stack, text};
use iced::{Alignment, Color, Element, Length, Padding, Point, Rectangle, Renderer, Size, Theme, mouse};

use std::collections::HashSet;
use std::path::PathBuf;

use crate::app::{App, Bundle, Entry, ExportWhat, Message, Open};
use crate::shelf::{self, Msg, Pick, Target};
use crate::i18n::{tr, trc, trf};
use crate::{theme, widgets};

pub fn view(app: &App) -> Element<'_, Message> {
    let imports = if app.bundles.is_empty() { tr("Importieren").to_string() } else { trf("Importieren · {}", &[&app.bundles.len()]) };
    let top = container(
        row![
            wordmark(),
            Space::new().width(Length::Fill),
            sync_chip(app),
            Space::new().width(12),
            button(theme::btn(tr("Einladung"))).padding(theme::pad()).on_press(Message::StartJoin).style(theme::tab(app.joining.is_some())),
            Space::new().width(8),
            button(theme::btn(&imports)).padding(theme::pad()).on_press(Message::ShowImports(!app.show_imports)).style(theme::tab(app.show_imports)),
            Space::new().width(8),
            button(theme::btn(tr("+ Neues Notizbuch"))).padding(theme::pad()).on_press(Message::StartCreate).style(theme::accent_button),
            Space::new().width(8),
            button(theme::btn(tr("Einstellungen"))).padding(theme::pad()).on_press(Message::ShowSettings(true)).style(theme::ghost),
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 8.0, bottom: 8.0, left: 20.0, right: 16.0 })
    .height(58)
    .width(Length::Fill)
    .style(theme::chrome);

    let s = &app.shelf;
    let shown = shelf::shown(&app.entries, &s.pick, &s.query, s.sort);
    let rows = shelf::folder_rows(app.entries.iter().map(|e| e.folder.as_str()), s.empty.iter().map(String::as_str));
    let searching = !s.query.trim().is_empty();
    // In a folder its subfolders come first, as tiles.
    let tiles = match &s.pick {
        Pick::Folder(f) if !searching => shelf::subfolders(&rows, f),
        _ => Vec::new(),
    };
    let home = s.pick == Pick::Home && !searching;
    let top_folders = shelf::subfolders(&rows, "");
    let head = if home { shelf::header(app, app.entries.len(), top_folders.len()) } else { shelf::header(app, shown.len(), tiles.len()) };
    let mut content = column![head].spacing(26).padding(Padding { top: 22.0, right: 32.0, bottom: 40.0, left: 32.0 });
    if let Some(r) = app.create_row() {
        content = content.push(r);
    }
    if let Some(link) = &app.joining {
        content = content.push(
            container(
                column![
                    theme::caption(tr("Einladung öffnen")),
                    row![
                        iced::widget::text_input("n0te://invite/…", link).on_input(Message::JoinLink).on_submit(Message::Join).padding(10).size(14).style(theme::field),
                        // The invite's QR code off another screen.
                        if crate::webcam::supported() {
                            Element::from(row![Space::new().width(8), button(theme::btn(tr("Mit der Kamera scannen"))).padding(theme::pad()).on_press(Message::Scan(crate::webcam::Msg::Open(crate::webcam::Purpose::Join))).style(theme::secondary)])
                        } else {
                            Space::new().into()
                        },
                        Space::new().width(8),
                        button(theme::btn(tr("Beitreten"))).padding(theme::pad()).on_press(Message::Join).style(theme::accent_button),
                        Space::new().width(6),
                        button(theme::btn(tr("Abbrechen"))).padding(theme::pad()).on_press(Message::CancelJoin).style(theme::ghost),
                    ]
                    .align_y(Alignment::Center),
                    text(tr("Das Notizbuch wird direkt vom Gerät geladen, das es geteilt hat, ohne Server.")).size(12).color(theme::p().muted),
                    // The invite asks for a PIN (whoever invited you tells it).
                    match app.join_pin.as_ref() {
                        Some(pin) => Element::from(
                            row![
                                text("PIN").size(13).color(theme::p().text),
                                iced::widget::text_input(tr("z. B. 4821"), pin).on_input(Message::JoinPin).on_submit(Message::Join).padding(10).size(14).width(Length::Fixed(160.0)).style(theme::field),
                                text(tr("Wer dich eingeladen hat, sagt sie dir.")).size(12).color(theme::p().muted),
                            ]
                            .spacing(10)
                            .align_y(Alignment::Center),
                        ),
                        None => Space::new().into(),
                    },
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
        content = content.push(row![imports_panel(&app.bundles), pdf_panel(&app.pdfs, tr("PDF importieren: wird ein neues Notizbuch"))].spacing(16));
    } else if app.show_pdf_import {
        content = content.push(pdf_panel(&app.pdfs, tr("PDF importieren: wird ein neues Notizbuch")));
    }
    if let Some(report) = &app.crash {
        content = content.push(crash_banner(report));
    }
    if app.entries.is_empty() && rows.is_empty() {
        content = content.push(
            container(
                column![
                    theme::heading(tr("Noch leer"), 22.0),
                    text(tr("Leg ein Notizbuch an, nimm eins vom Tablet (dort: Notizbuch halten → An den Laptop) oder importiere eine PDF.")).size(14).color(theme::p().muted),
                    row![
                        button(theme::btn(tr("+ Neues Notizbuch"))).padding(theme::pad()).on_press(Message::StartCreate).style(theme::accent_button),
                        Space::new().width(8),
                        button(theme::btn(tr("Importieren"))).padding(theme::pad()).on_press(Message::ShowImports(true)).style(theme::secondary),
                    ],
                ]
                .spacing(12),
            )
            .padding(24)
            .max_width(620)
            .style(theme::group),
        );
    }
    if home {
        if !(app.entries.is_empty() && rows.is_empty()) {
            content = content.push(start(app, &top_folders));
        }
    } else if shown.is_empty() && tiles.is_empty() && !app.entries.is_empty() {
        let hint = if searching {
            tr("Nichts gefunden")
        } else if matches!(s.pick, Pick::Folder(_)) {
            tr("Noch leer: zieh ein Notizbuch hierher oder nimm „Verschieben nach …“.")
        } else {
            tr("Nichts hier")
        };
        content = content.push(text(hint).size(14).color(theme::p().muted));
    }
    if !home && !tiles.is_empty() {
        let tiles: Vec<Element<'_, Message>> = tiles.into_iter().map(|r| shelf::folder_tile(app, r)).collect();
        content = content.push(iced::widget::Row::with_children(tiles).spacing(20).wrap().vertical_spacing(16));
    }
    if !home && !shown.is_empty() {
        // Where a notebook lies says something only outside its folder.
        let show_folder = searching || !matches!(s.pick, Pick::Folder(_));
        content = content.push(notebooks(app, shown, show_folder));
    }

    // The design's decorations (snow, stars, …) behind the shelf, under the scrolling content.
    let shelf: Element<'_, Message> = match crate::decor::layer(app.settings.decor, app.settings.decor_motion) {
        Some(decor) => iced::widget::Stack::new().push(App::scroll(content)).push_under(decor).into(),
        None => App::scroll(content).into(),
    };
    let main = column![shelf::toolbar(app), container(shelf).height(Length::Fill).style(theme::surface)];
    column![
        top,
        container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
        row![shelf::sidebar(app), container(Space::new()).width(1).height(Length::Fill).style(theme::hairline), main].height(Length::Fill),
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
    let label = if d.online { trf("{} verbunden", &[&d.name]) } else { trf("{} nicht erreichbar", &[&d.name]) };
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

/// n0te crashed last time: what went wrong, and buttons to copy or report it.
fn crash_banner(report: &str) -> Element<'_, Message> {
    container(
        column![
            text(tr("n0te ist beim letzten Mal abgestürzt")).font(theme::title_font()).size(15).color(theme::p().text),
            text(trf("Fehler: {}", &[&crate::crash::summary(report)])).font(theme::meta_font()).size(12).color(theme::p().muted),
            text(tr("Hilf beim Beheben: kopiere den Bericht und schick ihn mit, was du gerade getan hast. Er enthält keine Notizen.")).size(12.5).color(theme::p().muted),
            row![
                button(theme::btn_small(tr("Auf GitHub melden"))).padding(theme::pad()).on_press(Message::CrashReport).style(theme::accent_button),
                button(theme::btn_small(tr("Bericht kopieren"))).padding(theme::pad()).on_press(Message::CrashCopy).style(theme::secondary),
                button(theme::btn_small(tr("Schließen"))).padding(theme::pad()).on_press(Message::CrashDismiss).style(theme::ghost),
            ]
            .spacing(8),
        ]
        .spacing(6),
    )
    .padding(16)
    .max_width(720)
    .style(theme::panel)
    .into()
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

pub const COVER: (f32, f32) = (164.0, 212.0);
/// The covers in the start's "Zuletzt geöffnet".
const SMALL_COVER: (f32, f32) = (104.0, 135.0);
/// The covers in the list.
const ROW_COVER: (f32, f32) = (40.0, 52.0);

/// The start: what was opened last (a row), the pinned ones, the folders (with "Neuer
/// Ordner"), the notebooks in no folder. Everything else is a click away in the sidebar.
fn start<'a>(app: &'a App, folders: &[&shelf::FolderRow]) -> Element<'a, Message> {
    let s = &app.shelf;
    let mut col = column![].spacing(16);
    if app.entries.len() > 3 {
        let recent: Vec<Element<'a, Message>> = recent(app).into_iter().map(|e| card(app, e, true, true)).collect();
        col = col.push(shelf::section_head("Zuletzt geöffnet", tr("Zuletzt geöffnet"), None));
        col = col.push(
            iced::widget::scrollable(container(iced::widget::Row::with_children(recent).spacing(14)).padding(Padding { bottom: 10.0, ..Padding::ZERO }))
                .direction(iced::widget::scrollable::Direction::Horizontal(iced::widget::scrollable::Scrollbar::new().width(4).scroller_width(4)))
                .style(theme::scroll_dark),
        );
    }
    let pins = shelf::shown(&app.entries, &Pick::Pinned, "", s.sort);
    if !pins.is_empty() {
        col = col.push(container(shelf::section_head("Angeheftet", tr("Angeheftet"), Some(pins.len()))).padding(Padding { top: 10.0, ..Padding::ZERO }));
        col = col.push(notebooks(app, pins, true));
    }
    col = col.push(container(shelf::section_head("Ordner", tr("Ordner"), Some(folders.len()))).padding(Padding { top: 10.0, ..Padding::ZERO }));
    let mut tiles: Vec<Element<'a, Message>> = folders.iter().map(|r| shelf::folder_tile(app, r)).collect();
    tiles.push(shelf::new_folder_tile());
    col = col.push(iced::widget::Row::with_children(tiles).spacing(20).wrap().vertical_spacing(16));
    let loose: Vec<&Entry> = shelf::shown(&app.entries, &Pick::All, "", s.sort).into_iter().filter(|e| e.folder.is_empty() && !e.pinned).collect();
    if !loose.is_empty() {
        col = col.push(container(shelf::section_head("Ohne Ordner", tr("Ohne Ordner"), Some(loose.len()))).padding(Padding { top: 10.0, ..Padding::ZERO }));
        col = col.push(notebooks(app, loose, false));
    }
    col.into()
}

/// Up to eight notebooks, the one opened last first (never opened here: when it changed).
pub fn recent(app: &App) -> Vec<&Entry> {
    let opened: std::collections::HashMap<&str, u64> = app.settings.lib_opened.iter().map(|(id, t)| (id.as_str(), *t)).collect();
    let ms = |t: Option<SystemTime>| t.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
    let mut all: Vec<&Entry> = app.entries.iter().collect();
    all.sort_by_key(|e| std::cmp::Reverse(opened.get(e.id.as_str()).copied().unwrap_or_else(|| ms(e.modified))));
    all.truncate(8);
    all
}

/// Notebooks as cards (a wrapping grid) or as a list, as chosen.
fn notebooks<'a>(app: &'a App, es: Vec<&'a Entry>, show_folder: bool) -> Element<'a, Message> {
    if app.settings.lib_list {
        iced::widget::Column::with_children(es.into_iter().map(|e| notebook_row(app, e, show_folder))).spacing(6).into()
    } else {
        iced::widget::Row::with_children(es.into_iter().map(|e| card(app, e, show_folder, false))).spacing(20).wrap().vertical_spacing(22).into()
    }
}

/// What a click, a right click, ⋯ and a drag do with a notebook (card or row): (on press, the
/// menu's target). The menu is about the ticked notebooks when this is one of them.
fn handling(app: &App, e: &Entry) -> (Message, Target) {
    let s = &app.shelf;
    let press = if s.selected.is_some() { Message::Shelf(Msg::Tick(e.id.clone())) } else { Message::Open(e.id.clone()) };
    let target = match &s.selected {
        Some(sel) if sel.len() > 1 && sel.contains(&e.id) => Target::Notebooks(app.entries.iter().filter(|x| sel.contains(&x.id)).map(|x| x.id.clone()).collect()),
        _ => Target::Notebooks(vec![e.id.clone()]),
    };
    (press, target)
}

/// ⋯: opens the menu where it is.
fn dots<'a>(target: Target, framed: bool) -> Element<'a, Message> {
    let mark = container(text("⋯").size(16).line_height(1.0).color(theme::p().text)).padding([2, 8]);
    let mark = if framed { mark.style(theme::chip) } else { mark };
    widgets::pointer(mouse_area(mark).interaction(iced::mouse::Interaction::Pointer)).on_left(move |at| Message::Shelf(Msg::Menu(target.clone(), at))).into()
}

/// "12 Seiten · vor 2 h", with where it lies first when that helps ("WS26 · …"; the whole
/// way in the list: "Uni › WS26 · …").
fn meta(e: &Entry, show_folder: bool, whole_way: bool) -> String {
    let mut meta = if e.sheets == 1 { trf("{} Seite", &[&e.sheets]) } else { trf("{} Seiten", &[&e.sheets]) };
    if let Some(t) = e.modified {
        meta += &format!(" · {}", age(t));
    }
    if show_folder && !e.folder.is_empty() {
        let way = if whole_way { e.folder.replace('/', " › ") } else { shelf::name(&e.folder).to_string() };
        meta = format!("{way} · {meta}");
    }
    theme::cased(&meta)
}

fn meta_size() -> f32 {
    if theme::s().label == theme::Label::MonoCaps { theme::LABEL } else { 12.0 }
}

/// A notebook card: its cover, title (a pin mark when pinned), tags, pages and age. A click
/// opens it (ticks it in "Auswählen"), a right click or ⋯ (on the cover, under the pointer)
/// opens its menu, dragging it onto a folder moves it there. Small: in "Zuletzt geöffnet".
fn card<'a>(app: &'a App, e: &'a Entry, show_folder: bool, small: bool) -> Element<'a, Message> {
    let ticked = app.shelf.selected.as_ref().map(|sel| sel.contains(&e.id));
    let (press, target) = handling(app, e);
    let (w, h) = if small { SMALL_COVER } else { COVER };
    let cover: Element<'a, Message> = match ticked {
        Some(on) => stack![cover_preview(&e.cover, &e.title, w, h), container(check(on)).padding(8)].into(),
        None => cover_preview(&e.cover, &e.title, w, h),
    };
    let cover = hover(cover, container(dots(target.clone(), true)).align_right(Length::Fill).padding(if small { 5 } else { 7 }));
    let chars = if small { 16 } else { 21 };
    let mut title = row![
        text(fit(&e.title, if e.pinned { chars - 2 } else { chars })).font(theme::title_font()).size(if small { 13.0 } else { 15.0 }).color(theme::p().text).wrapping(text::Wrapping::None),
        Space::new().width(Length::Fill)
    ]
    .align_y(Alignment::Center);
    if e.pinned {
        title = title.push(shelf::icon(shelf::PIN, if small { 12.0 } else { 14.0 }, theme::p().accent));
    }
    let mut col = column![cover, Space::new().height(if small { 7 } else { 10 }), title].spacing(3);
    if !e.tags.is_empty() && !small {
        col = col.push(container(shelf::tag_chips(&e.tags)).padding([2, 0]));
    }
    // Small: where it lies (or how big it is), no more.
    let facts = if small && !e.folder.is_empty() { theme::cased(shelf::name(&e.folder)) } else if small { theme::cased(&if e.sheets == 1 { trf("{} Seite", &[&e.sheets]) } else { trf("{} Seiten", &[&e.sheets]) }) } else { meta(e, show_folder, false) };
    col = col.push(text(fit(&facts, if small { 18 } else { 26 })).font(theme::meta_font()).size(meta_size()).color(theme::p().muted).wrapping(text::Wrapping::None));
    let pad = if small { 8.0 } else { 10.0 };
    let b = button(col).padding(pad).width(w + 2.0 * pad).on_press(press).style(theme::card_ticked(ticked == Some(true)));
    let id = e.id.clone();
    let card = widgets::pointer(b).on_right(move |at| Message::Shelf(Msg::Menu(target.clone(), at))).on_drag(move |d| Message::Shelf(Msg::Drag(id.clone(), d)));
    container(card).id(if small { shelf::recent_id(&e.id) } else { shelf::card_id(&e.id) }).into()
}

/// A notebook as a row of the list: small cover, title (pin before it), where it lies and how
/// big, up to four tags, ⋯. Clicks, right click, drag and ticking work as on a card.
fn notebook_row<'a>(app: &'a App, e: &'a Entry, show_folder: bool) -> Element<'a, Message> {
    let ticked = app.shelf.selected.as_ref().map(|sel| sel.contains(&e.id));
    let (press, target) = handling(app, e);
    let mut line = row![].spacing(14).align_y(Alignment::Center);
    if let Some(on) = ticked {
        line = line.push(check(on));
    }
    line = line.push(cover_preview(&e.cover, &e.title, ROW_COVER.0, ROW_COVER.1));
    let mut title = row![].spacing(6).align_y(Alignment::Center);
    if e.pinned {
        title = title.push(shelf::icon(shelf::PIN, 13.0, theme::p().accent));
    }
    title = title.push(text(fit(&e.title, 48)).font(theme::title_font()).size(15).color(theme::p().text).wrapping(text::Wrapping::None));
    line = line.push(column![title, text(fit(&meta(e, show_folder, true), 64)).font(theme::meta_font()).size(meta_size()).color(theme::p().muted).wrapping(text::Wrapping::None)].spacing(3));
    line = line.push(Space::new().width(Length::Fill));
    for t in e.tags.iter().take(4) {
        line = line.push(row![shelf::dot(shelf::tag_colour(t), 8.0), text(fit(t, 14)).size(12).color(theme::p().muted).wrapping(text::Wrapping::None)].spacing(5).align_y(Alignment::Center));
    }
    line = line.push(dots(target.clone(), false));
    let b = button(line).padding([8, 12]).width(Length::Fill).on_press(press).style(theme::card_ticked(ticked == Some(true)));
    let id = e.id.clone();
    let row = widgets::pointer(b).on_right(move |at| Message::Shelf(Msg::Menu(target.clone(), at))).on_drag(move |d| Message::Shelf(Msg::Drag(id.clone(), d)));
    container(row).id(shelf::card_id(&e.id)).into()
}

/// The tick box on a card in "Auswählen".
fn check<'a>(on: bool) -> Element<'a, Message> {
    let mark: Element<'a, Message> = if on { shelf::icon(shelf::CHECK, 14.0, Color::WHITE) } else { Space::new().into() };
    container(mark)
        .center_x(22)
        .center_y(22)
        .style(move |_| container::Style {
            background: Some((if on { theme::p().accent } else { Color { a: 0.88, ..Color::WHITE } }).into()),
            border: iced::Border { color: if on { theme::p().accent } else { theme::alpha(theme::INK, 0.45) }, width: 1.5, radius: 6.0.into() },
            ..Default::default()
        })
        .into()
}

/// A notebook's cover at any size (cards, the cover dialog).
pub fn cover_preview<'a>(kind: &str, title: &str, w: f32, h: f32) -> Element<'a, Message> {
    canvas_widget(Cover { kind: kind.to_string(), title: title.to_string() }).width(w).height(h).into()
}

fn imports_panel(bundles: &[Bundle]) -> Element<'_, Message> {
    let mut list = column![theme::caption(tr("Vom Tablet (~/Downloads/n0bridge)"))].spacing(8);
    if bundles.is_empty() {
        list = list.push(text(tr("Keine .n0te-Dateien gefunden. Auf dem Tablet: Notizbuch halten → An den Laptop.")).size(13).color(theme::p().muted));
    }
    for b in bundles {
        let name = b.path.file_stem().map(|s| s.to_string_lossy().replace('_', " ")).unwrap_or_default();
        list = list.push(
            row![
                column![text(name).size(14).color(theme::p().text), text(age(b.modified)).font(theme::meta_font()).size(11.5).color(theme::p().muted)].spacing(1),
                Space::new().width(Length::Fill),
                button(theme::btn_small(trc("bundle", "Übernehmen"))).padding(theme::pad()).on_press(Message::Import(b.path.clone())).style(theme::secondary),
            ]
            .align_y(Alignment::Center),
        );
    }
    list = list.push(text(tr("Übernehmen führt mit dem Stand auf dem Laptop zusammen: nichts geht verloren.")).size(12).color(theme::p().muted));
    container(list).padding(18).width(Length::Fill).style(theme::panel).into()
}

/// Recent PDFs to import; dropping a file onto the window works too.
pub fn pdf_panel<'a>(pdfs: &'a [(PathBuf, SystemTime)], title: &'a str) -> Element<'a, Message> {
    let mut list = column![theme::caption(title)].spacing(8);
    if pdfs.is_empty() {
        list = list.push(text(tr("Keine PDFs in Downloads oder Dokumente.")).size(13).color(theme::p().muted));
    }
    for (path, modified) in pdfs.iter().take(12) {
        let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let dir = path.parent().and_then(|d| d.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        list = list.push(
            row![
                column![text(fit(&name, 52)).size(14).color(theme::p().text), text(format!("{dir} · {}", age(*modified))).font(theme::meta_font()).size(11.5).color(theme::p().muted)].spacing(1),
                Space::new().width(Length::Fill),
                button(theme::btn_small(tr("Nehmen"))).padding(theme::pad()).on_press(Message::ImportPdf(path.clone())).style(theme::secondary),
            ]
            .align_y(Alignment::Center),
        );
    }
    list = list.push(text(tr("Oder eine PDF- bzw. .n0te-Datei ins Fenster ziehen.")).size(12).color(theme::p().muted));
    container(list).padding(18).width(Length::Fill).max_width(900).style(theme::panel).into()
}

/// Export as a dialog: pick the scope (whole notebook, this page, a selection), where the side
/// pages go (`sides_at_end`), then one primary action. Selected pages take their side pages along.
pub fn export_dialog<'a>(o: &'a Open, picked: &'a HashSet<String>, mode: ExportWhat, current: u32, sides_at_end: bool) -> Element<'a, Message> {
    let tile = |what: ExportWhat, title: &str, sub: String| {
        button(column![text(title.to_string()).font(theme::title_font()).size(15).color(theme::p().text), text(sub).size(12).color(theme::p().muted)].spacing(4))
            .padding([14, 16])
            .width(Length::Fill)
            .on_press(Message::ExportMode(what))
            .style(theme::option(mode == what))
    };
    let mut col = column![
        row![theme::heading(tr("PDF exportieren"), 22.0), Space::new().width(Length::Fill), button(theme::btn_small(tr("Schließen"))).padding(theme::pad()).on_press(Message::ShowExport(false)).style(theme::ghost)].align_y(Alignment::Center),
        theme::caption(tr("Umfang")),
        row![
            tile(ExportWhat::All, tr("Ganzes Notizbuch"), trf("{} Seiten mit Seitenblättern", &[&o.pages.total_pages])),
            tile(ExportWhat::Current, tr("Diese Seite"), trf("Seite {} und ihre Seitenblätter", &[&current])),
            tile(ExportWhat::Picked, tr("Auswahl"), if picked.is_empty() { tr("Seiten unten antippen").to_string() } else { if picked.len() == 1 { trf("{} Seite gewählt", &[&picked.len()]) } else { trf("{} Seiten gewählt", &[&picked.len()]) } }),
        ]
        .spacing(10),
    ]
    .spacing(14);
    if mode == ExportWhat::Picked {
        let mut chips = Vec::new();
        // Every page on its own, side pages too ("03 L1", "03 R2"), in reading order.
        let mut rows: Vec<&n0te_core::layout::Placed> = o.pages.pages.iter().map(|p| &p.placed).collect();
        rows.sort_by_key(|p| (p.number, p.side));
        for p in rows {
            let on = picked.contains(&p.node);
            let label = match p.side {
                0 => format!("{:02}", p.number),
                s if s < 0 => format!("{:02} L{}", p.number, -s),
                s => format!("{:02} R{}", p.number, s),
            };
            chips.push(button(text(label).font(theme::MONO).size(12)).padding([6, 11]).on_press(Message::TogglePick(p.node.clone())).style(theme::tab(on)).into());
        }
        col = col.push(theme::caption(tr("Seiten (L/R: Seitenblätter, einzeln wählbar)")));
        col = col.push(iced::widget::scrollable(iced::widget::Row::with_children(chips).spacing(6).wrap().vertical_spacing(6)).style(theme::scroll_dark).height(Length::Fixed(190.0)));
    }
    let order = |at_end: bool, label: &str| button(text(label.to_string()).size(12)).padding([6, 11]).on_press(Message::ExportSidesAtEnd(at_end)).style(theme::tab(sides_at_end == at_end));
    col = col.push(
        row![
            theme::caption(tr("Seitenblätter")),
            Space::new().width(Length::Fill),
            order(false, tr("direkt nach ihrer Seite")),
            order(true, tr("am Ende")),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    );
    let ready = mode != ExportWhat::Picked || !picked.is_empty();
    col = col.push(container(Space::new()).height(1).width(Length::Fill).style(theme::hairline));
    col = col.push(
        row![
            text(tr("Nach ~/Downloads · Seiten bleiben weiß, auch in dunklen Designs")).font(theme::meta_font()).size(12).color(theme::p().muted),
            Space::new().width(Length::Fill),
            button(theme::btn(tr("Abbrechen"))).padding(theme::pad()).on_press(Message::ShowExport(false)).style(theme::ghost),
            Space::new().width(8),
            button(theme::btn(tr("Exportieren"))).padding(theme::pad()).on_press_maybe(ready.then_some(Message::Export(mode))).style(theme::accent_button),
        ]
        .align_y(Alignment::Center),
    );
    container(container(col).padding(24).width(Length::Fixed(700.0)).style(theme::panel)).padding(Padding { top: 80.0, ..Padding::ZERO }).center_x(Length::Fill).into()
}

fn age(t: SystemTime) -> String {
    let s = SystemTime::now().duration_since(t).map(|d| d.as_secs()).unwrap_or(0);
    match s {
        0..=59 => tr("gerade eben").into(),
        60..=3599 => trf("vor {} min", &[&(s / 60)]),
        3600..=86_399 => trf("vor {} h", &[&(s / 3600)]),
        _ => trf("vor {} Tagen", &[&(s / 86_400)]),
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
