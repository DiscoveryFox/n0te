//! Small custom widgets: a drag grip (move / resize text boxes), Escape on a text field, room
//! left below a part of a column (a dialog's buttons under a scrolling body).

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::text::{self, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse, overlay};
use iced::keyboard::{self, Key, key::Named};
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

/// A text field that also reports Escape: the field itself only lets go of the focus (and keeps
/// the key from the app's shortcuts), this sends `on_escape` too — while it had the focus.
pub struct OnEscape<'a, Message> {
    content: Element<'a, Message>,
    on_escape: Message,
}

pub fn on_escape<'a, Message>(content: impl Into<Element<'a, Message>>, on_escape: Message) -> OnEscape<'a, Message> {
    OnEscape { content: content.into(), on_escape }
}

impl<'a, Message: Clone> Widget<Message, Theme, iced::Renderer> for OnEscape<'a, Message> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn Operation) {
        self.content.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &iced::Renderer, clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, viewport: &Rectangle) {
        let escape = matches!(event, Event::Keyboard(keyboard::Event::KeyPressed { key: Key::Named(Named::Escape), .. })) && !shell.is_event_captured();
        self.content.as_widget_mut().update(&mut tree.children[0], event, layout, cursor, renderer, clipboard, shell, viewport);
        // A text field takes Escape only while it has the focus.
        if escape && shell.is_event_captured() {
            shell.publish(self.on_escape.clone());
        }
    }
    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &iced::Renderer) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }
    fn draw(&self, tree: &Tree, renderer: &mut iced::Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) {
        self.content.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }
    fn overlay<'b>(&'b mut self, tree: &'b mut Tree, layout: Layout<'b>, renderer: &iced::Renderer, viewport: &Rectangle, translation: Vector) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(&mut tree.children[0], layout, renderer, viewport, translation)
    }
}

impl<'a, Message: Clone + 'a> From<OnEscape<'a, Message>> for Element<'a, Message> {
    fn from(w: OnEscape<'a, Message>) -> Self {
        Element::new(w)
    }
}

/// `content` laid out with `below` pixels less height than it is offered: in a column, the
/// rows after it keep that much room (a scrolling dialog body over the dialog's buttons — a
/// column gives its rows the height in order, so a tall body would push them out).
pub struct LeaveRoom<'a, Message> {
    content: Element<'a, Message>,
    below: f32,
}

pub fn leave_room<'a, Message>(content: impl Into<Element<'a, Message>>, below: f32) -> LeaveRoom<'a, Message> {
    LeaveRoom { content: content.into(), below }
}

impl<'a, Message> Widget<Message, Theme, iced::Renderer> for LeaveRoom<'a, Message> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        let max = limits.max().height;
        let limits = if max.is_finite() { limits.max_height((max - self.below).max(0.0)) } else { *limits };
        self.content.as_widget_mut().layout(&mut tree.children[0], renderer, &limits)
    }
    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn Operation) {
        self.content.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &iced::Renderer, clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, viewport: &Rectangle) {
        self.content.as_widget_mut().update(&mut tree.children[0], event, layout, cursor, renderer, clipboard, shell, viewport);
    }
    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &iced::Renderer) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }
    fn draw(&self, tree: &Tree, renderer: &mut iced::Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) {
        self.content.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }
    fn overlay<'b>(&'b mut self, tree: &'b mut Tree, layout: Layout<'b>, renderer: &iced::Renderer, viewport: &Rectangle, translation: Vector) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(&mut tree.children[0], layout, renderer, viewport, translation)
    }
}

impl<'a, Message: 'a> From<LeaveRoom<'a, Message>> for Element<'a, Message> {
    fn from(w: LeaveRoom<'a, Message>) -> Self {
        Element::new(w)
    }
}
