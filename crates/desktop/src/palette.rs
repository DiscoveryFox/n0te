//! The command palette (Ctrl+K): every action and every page/notebook by name, filtered as
//! you type; Up/Down choose, Enter runs.

use iced::widget::{Space, button, column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Padding};

use crate::app::{App, Message, Screen};
use crate::i18n::{tr, trf};
use crate::theme;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    NewNotebook,
    ImportPdf,
    ImportBundle,
    OpenNotebook(String),
    Library,
    NewTextPage,
    Notes,
    NewTextBox,
    /// A coordinate system on the page in view (its dialog opens).
    InsertPlot,
    InsertPdf,
    Export,
    SendToTablet,
    ToggleSidebar,
    Zoom(f32),
    GoTo(String),
    Theme(&'static str),
    Settings,
    Help,
    History,
    Share,
    Live,
    /// The page in view: turn it (portrait ↔ landscape); its paper and format.
    RotatePage,
    PageSetup,
    /// The outline's search field (Ctrl+F).
    Search,
    /// Two side by side: this notebook again beside it, left and right swapped, the other one
    /// closed.
    Again,
    Swap,
    CloseOther,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub label: String,
    pub hint: String,
    pub cmd: Command,
}

fn item(label: impl Into<String>, hint: impl Into<String>, cmd: Command) -> Item {
    Item { label: label.into(), hint: hint.into(), cmd }
}

/// What the palette offers on the current screen.
pub fn items(app: &App) -> Vec<Item> {
    let mut v = Vec::new();
    match &app.screen {
        Screen::Library => {
            v.push(item(tr("Neues Notizbuch"), tr("Strg N"), Command::NewNotebook));
            v.push(item(tr("PDF importieren"), tr("Strg O · wird ein neues Notizbuch"), Command::ImportPdf));
            v.push(item(tr("Vom Tablet übernehmen (.n0te)"), tr("Strg I"), Command::ImportBundle));
            for e in &app.entries {
                let folder = if e.folder.is_empty() { String::new() } else { format!(" · {}", e.folder) };
                v.push(item(trf("Öffnen: {}", &[&e.title]), format!("{}{folder}", trf("{} Seiten", &[&e.sheets])), Command::OpenNotebook(e.id.clone())));
            }
        }
        Screen::Notebook(o) => {
            v.push(item(tr("Suchen"), tr("Strg F · Handschrift, Text, Überschriften, PDF"), Command::Search));
            v.push(item(tr("Neue Textseite"), tr("Strg N · nach der aktuellen Seite"), Command::NewTextPage));
            v.push(item(tr("Notizen neben der Seite"), tr("Strg M"), Command::Notes));
            v.push(item(tr("Textfeld"), tr("Strg T · oder Doppelklick auf die Seite"), Command::NewTextBox));
            v.push(item(tr("Koordinatensystem einfügen"), tr("Funktionsgraphen · mitten auf die Seite"), Command::InsertPlot));
            v.push(item(tr("PDF einfügen"), tr("Strg O · nach der aktuellen Seite"), Command::InsertPdf));
            v.push(item(tr("Seite drehen"), tr("Strg R · hoch ↔ quer"), Command::RotatePage));
            v.push(item(tr("Papier und Format der Seite"), tr("A4, A5, eigene Größe · oder oben auf die Seitenzahl"), Command::PageSetup));
            v.push(item(tr("PDF exportieren"), tr("Strg E"), Command::Export));
            if crate::app::n0bridge_installed() {
                v.push(item(tr("An das Tablet senden"), tr("über n0bridge (.n0te)"), Command::SendToTablet));
            }
            v.push(item(tr("Live mit dem Tablet"), tr("es öffnet das Notizbuch, Folgen, Laserpointer"), Command::Live));
            v.push(item(tr("Teilen / Gäste einladen"), tr("nur lesen oder lesen & schreiben"), Command::Share));
            v.push(item(tr("Verlauf: früheren Stand wiederherstellen"), tr("Strg H"), Command::History));
            v.push(item(tr("Gliederung ein/aus"), tr("Strg B"), Command::ToggleSidebar));
            v.push(item(tr("Dieses Notizbuch daneben"), tr("zwei Stellen zugleich · ⧉ in der Leiste"), Command::Again));
            if let Screen::Notebook(s) = &app.screen
                && s.panes.len() > 1
            {
                v.push(item(tr("Seiten tauschen"), tr("die zwei Notizbücher"), Command::Swap));
                v.push(item(tr("Den anderen schließen"), "Esc", Command::CloseOther));
            }
            v.push(item("Zoom 100 %", tr("Strg 0"), Command::Zoom(1.0)));
            v.push(item("Zoom 200 %", "", Command::Zoom(2.0)));
            v.push(item(tr("Bibliothek"), "Esc", Command::Library));
            for p in o.pages.pages.iter().map(|p| &p.placed) {
                let title = o
                    .rows
                    .iter()
                    .find_map(|r| match r {
                        n0te_core::notebook::OutlineRow::Sheet { node, title, .. } if *node == p.node => Some(title.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                let kind = if p.spec.pdf.is_some() { "PDF" } else if p.spec.background == n0te_core::notebook::Background::Text { tr("Text") } else if p.is_board() { "Board" } else { tr("Seite") };
                v.push(item(trf("Seite {} {}", &[&p.label(), &title]).trim().to_string(), kind, Command::GoTo(p.sheet_id.clone())));
            }
        }
    }
    v.push(item(tr("Einstellungen"), tr("Strg ,"), Command::Settings));
    v.push(item(tr("Tastenkürzel"), "F1", Command::Help));
    // The designs last: there are many, and the palette lists only its first rows unfiltered.
    for t in theme::all() {
        v.push(item(trf("Design: {}", &[&t.title()]), if t.id == theme::p().id { tr("aktiv") } else { "" }, Command::Theme(t.id.as_str())));
    }
    v
}

/// Items whose label+hint contain every word of the query (case-insensitive), best first.
pub fn filter(all: Vec<Item>, query: &str) -> Vec<Item> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    let mut hits: Vec<(usize, Item)> = all
        .into_iter()
        .filter_map(|it| {
            let hay = format!("{} {}", it.label, it.hint).to_lowercase();
            if !words.iter().all(|w| hay.contains(w.as_str())) {
                return None;
            }
            // Earlier matches in the label rank higher.
            let rank = words.first().and_then(|w| it.label.to_lowercase().find(w.as_str())).unwrap_or(999);
            Some((rank, it))
        })
        .collect();
    hits.sort_by_key(|(r, _)| *r);
    hits.into_iter().map(|(_, it)| it).collect()
}

pub fn input_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-palette")
}

pub fn view<'a>(query: &'a str, items: &[Item], selected: usize) -> Element<'a, Message> {
    let mut list = column![].spacing(2);
    for (i, it) in items.iter().take(12).enumerate() {
        let on = i == selected.min(items.len().saturating_sub(1));
        list = list.push(
            button(row![text(it.label.clone()).size(14), Space::new().width(Length::Fill), text(it.hint.clone()).font(theme::MONO).size(theme::LABEL).color(theme::p().muted)].align_y(Alignment::Center))
                .on_press(Message::PaletteRun(Some(i)))
                .padding([7, 12])
                .width(Length::Fill)
                .style(theme::row(on)),
        );
    }
    if items.is_empty() {
        list = list.push(text(tr("Nichts gefunden")).size(13).color(theme::p().muted));
    }
    let input = text_input(tr("Befehl, Seite oder Notizbuch …"), query).id(input_id()).on_input(Message::PaletteQuery).on_submit(Message::PaletteRun(None)).padding(12).size(16);
    let card = container(column![input, list].spacing(8)).padding(10).width(Length::Fixed(640.0)).style(theme::panel);
    container(card).padding(Padding { top: 80.0, ..Padding::ZERO }).center_x(Length::Fill).height(Length::Fill).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_needs_every_word_and_ranks_label_hits_first() {
        let all = vec![item("PDF exportieren", "Strg E", Command::Export), item("Neue Textseite", "PDF", Command::NewTextPage), item("Zoom 100 %", "", Command::Zoom(1.0))];
        let hits = filter(all.clone(), "pdf");
        assert_eq!(hits.iter().map(|i| i.cmd.clone()).collect::<Vec<_>>(), vec![Command::Export, Command::NewTextPage]);
        assert!(filter(all.clone(), "pdf zoom").is_empty());
        assert_eq!(filter(all, "").len(), 3);
    }
}
