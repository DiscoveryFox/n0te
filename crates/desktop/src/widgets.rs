//! Small custom widgets: a drag grip (move / resize text boxes).

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::text::{self, Renderer as _};
use iced::advanced::widget::{self, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::{Border, Color, Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

use crate::theme;

/// A small handle that reports drag movement (screen pixels) and the end of the drag.
pub struct Grip<'a, Message> {
    label: &'static str,
    on_drag: Box<dyn Fn(Vector) -> Message + 'a>,
    on_end: Message,
    interaction: mouse::Interaction,
}

pub fn grip<'a, Message>(label: &'static str, interaction: mouse::Interaction, on_drag: impl Fn(Vector) -> Message + 'a, on_end: Message) -> Grip<'a, Message> {
    Grip { label, on_drag: Box::new(on_drag), on_end, interaction }
}

#[derive(Default)]
struct State {
    last: Option<Point>,
}

const SIZE: f32 = 24.0;

impl<'a, Message: Clone> Widget<Message, Theme, iced::Renderer> for Grip<'a, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(SIZE), Length::Fixed(SIZE))
    }
    fn layout(&mut self, _tree: &mut widget::Tree, _renderer: &iced::Renderer, _limits: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(SIZE, SIZE))
    }

    fn update(&mut self, tree: &mut widget::Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, _renderer: &iced::Renderer, _clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, _viewport: &Rectangle) {
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(p) = cursor.position_over(layout.bounds()) {
                    state.last = Some(p);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(last) = state.last {
                    state.last = Some(*position);
                    shell.publish((self.on_drag)(*position - last));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.last.take().is_some() {
                    shell.publish(self.on_end.clone());
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(&self, tree: &widget::Tree, layout: Layout<'_>, cursor: mouse::Cursor, _viewport: &Rectangle, _renderer: &iced::Renderer) -> mouse::Interaction {
        if tree.state.downcast_ref::<State>().last.is_some() || cursor.is_over(layout.bounds()) { self.interaction } else { mouse::Interaction::None }
    }

    fn draw(&self, _tree: &widget::Tree, renderer: &mut iced::Renderer, _theme: &Theme, _style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) {
        let b = layout.bounds();
        let hover = cursor.is_over(b);
        renderer.fill_quad(
            renderer::Quad { bounds: b, border: Border { color: theme::p().accent, width: 1.0, radius: 6.0.into() }, ..Default::default() },
            if hover { theme::p().accent } else { Color::WHITE },
        );
        renderer.fill_text(
            text::Text {
                content: self.label.to_string(),
                bounds: b.size(),
                size: 13.0.into(),
                line_height: text::LineHeight::default(),
                font: theme::SANS_BOLD,
                align_x: text::Alignment::Center,
                align_y: iced::alignment::Vertical::Center,
                shaping: text::Shaping::Advanced,
                wrapping: text::Wrapping::None,
            },
            b.center(),
            if hover { Color::WHITE } else { theme::p().accent },
            *viewport,
        );
    }
}

impl<'a, Message: Clone + 'a> From<Grip<'a, Message>> for Element<'a, Message> {
    fn from(g: Grip<'a, Message>) -> Self {
        Element::new(g)
    }
}
