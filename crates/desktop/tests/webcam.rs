//! "Mit der Kamera scannen" through the headless harness: the button in the join dialog opens
//! the camera panel, what the camera thread would report goes through the app (the harness
//! runs no subscriptions, so no camera is ever opened): an invite fills the join dialog, a pair
//! code takes the pairing path, another code only gets a hint, Esc closes.

mod harness;

use harness::Harness;
use iced::Size;
use iced::keyboard::key::Named;
use n0te_desktop::app::Message;
use n0te_desktop::i18n::{self, Lang};
use n0te_desktop::webcam::{self, Event, Msg, Purpose};
use n0te_desktop::{App, theme};

fn library(tag: &str) -> Harness {
    i18n::set(Lang::De);
    let root = std::env::temp_dir().join(format!("n0te-cam-{tag}-{}-{}", std::process::id(), n0te_core::ids::new_id(9)));
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).expect("tiny-skia renderer");
    h.frame();
    h
}

/// Where `label` shows, in whatever case the design writes it.
fn find(h: &mut Harness, label: &str) -> Option<iced::Rectangle> {
    [label.to_string(), theme::cased(label), label.to_uppercase()].iter().find_map(|l| h.find(l.as_str()))
}

fn escape(h: &mut Harness) {
    let iced::Event::Keyboard(e) = h.key_event(iced::keyboard::Key::Named(Named::Escape), None)[0].clone() else { unreachable!() };
    h.message(Message::Key(e));
}

/// An event from the open panel's camera thread (no frame drawn: `find` lays the UI out, and a
/// debug-build frame takes long enough for the hint's few seconds to run out).
fn from_camera(h: &mut Harness, e: Event) {
    let session = h.app.scan.as_ref().expect("panel open").session();
    h.message(Message::Scan(Msg::Event(session, e)));
}

const INVITE: &str = "n0te://invite/AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEy";

#[test]
fn the_join_dialog_scans_an_invite() {
    let mut h = library("join");
    h.message(Message::StartJoin);
    h.frame();
    let Some(button) = find(&mut h, "Mit der Kamera scannen") else {
        assert!(!webcam::supported(), "the button shows where a camera can be used");
        return;
    };
    h.click(button.center());
    h.frame();
    assert!(h.app.scan.is_some(), "the panel is open");
    assert!(find(&mut h, "Halte den QR-Code der Einladung vor die Kamera.").is_some());
    assert!(find(&mut h, "Suche Kameras …").is_some());
    // Two cameras: both offered, the back one picked; a picture shows.
    let cams = vec![
        webcam::CameraInfo { id: "f".into(), name: "Microsoft Camera Front".into(), facing: webcam::Facing::Front },
        webcam::CameraInfo { id: "b".into(), name: "Microsoft Camera Rear".into(), facing: webcam::Facing::Back },
    ];
    from_camera(&mut h, Event::Cameras(cams, Some(1)));
    assert!(find(&mut h, "Hinten · Microsoft Camera Rear").is_some() && find(&mut h, "Vorne · Microsoft Camera Front").is_some());
    let grey: Vec<u8> = (0..320 * 240).flat_map(|i| [(i % 256) as u8, 90, 160, 255]).collect();
    from_camera(&mut h, Event::Frame(iced::widget::image::Handle::from_rgba(320, 240, grey)));
    h.frame();
    assert!(find(&mut h, "Kamera startet …").is_none(), "the picture replaces the waiting text");
    // Some other QR code: a hint, the panel stays.
    from_camera(&mut h, Event::Found("https://example.com".into()));
    assert!(find(&mut h, "Das ist kein n0te-Code").is_some());
    // The invite: the panel closes, the link is in the join dialog.
    from_camera(&mut h, Event::Found(INVITE.into()));
    assert!(h.app.scan.is_none());
    assert_eq!(h.app.joining.as_deref(), Some(INVITE));
}

#[test]
fn a_pair_code_takes_the_pairing_path_and_esc_closes() {
    let mut h = library("pair");
    h.message(Message::Scan(Msg::Open(Purpose::Pair)));
    h.frame();
    assert!(find(&mut h, "Halte den Koppel-Code des anderen Geräts vor die Kamera.").is_some());
    escape(&mut h);
    assert!(h.app.scan.is_none(), "Esc closes the panel");
    h.message(Message::Scan(Msg::Open(Purpose::Pair)));
    let code = "n0te-pair:AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyAhIiMkJSYnKCkqKywtLi8w";
    from_camera(&mut h, Event::Found(code.into()));
    assert!(h.app.scan.is_none());
    // Without a sync service here the pairing itself fails, but it was tried with the code.
    assert_eq!(h.app.pair_enter, code);
    assert!(h.app.status.as_deref().is_some_and(|s| s.starts_with("Koppeln: ")), "{:?}", h.app.status);
    // A failure shows why, with a way to try again; "Abbrechen" closes.
    h.message(Message::Scan(Msg::Open(Purpose::Pair)));
    from_camera(&mut h, Event::Failed("Keine Kamera gefunden".into()));
    assert!(find(&mut h, "Keine Kamera gefunden").is_some());
    let again = find(&mut h, "Nochmal versuchen").expect("retry button");
    h.click(again.center());
    h.frame();
    assert!(h.app.scan.as_ref().is_some_and(|s| s.failed.is_none()));
    let cancel = find(&mut h, "Abbrechen").expect("cancel button");
    h.click(cancel.center());
    h.frame();
    assert!(h.app.scan.is_none());
}
