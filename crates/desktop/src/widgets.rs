//! Small custom widgets: a drag grip (move / resize text boxes), Escape on a text field, room
//! left below a part of a column (a dialog's buttons under a scrolling body), the pointer on a
//! library card (right click, drag, drop), the layer that closes a menu, and the split's parts:
//! a pane that notices it is used, the divider between two panes.

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

/// What dragging a library card reports: it started (past a few pixels), the pointer moved,
/// it ended (positions in the window).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Drag {
    Start(Point),
    Move(Point),
    End,
}

/// How far the pointer moves with the button held before a press becomes a drag.
const DRAG_START: f32 = 6.0;

/// A card or a row in the library. A right click (and, with `on_left`, a left one) reports
/// where it happened in the window; with `on_drag`, holding the button and moving reports a
/// drag; as a drop target (`drop`, while something is dragged) it says when the pointer comes
/// over it and when it leaves. Plain clicks go on to the content (a button).
pub struct Pointer<'a, Message> {
    content: Element<'a, Message>,
    on_right: Option<Box<dyn Fn(Point) -> Message + 'a>>,
    on_left: Option<Box<dyn Fn(Point) -> Message + 'a>>,
    on_drag: Option<Box<dyn Fn(Drag) -> Message + 'a>>,
    drop: Option<(Message, Message)>,
}

pub fn pointer<'a, Message>(content: impl Into<Element<'a, Message>>) -> Pointer<'a, Message> {
    Pointer { content: content.into(), on_right: None, on_left: None, on_drag: None, drop: None }
}

impl<'a, Message> Pointer<'a, Message> {
    pub fn on_right(mut self, f: impl Fn(Point) -> Message + 'a) -> Self {
        self.on_right = Some(Box::new(f));
        self
    }
    pub fn on_left(mut self, f: impl Fn(Point) -> Message + 'a) -> Self {
        self.on_left = Some(Box::new(f));
        self
    }
    pub fn on_drag(mut self, f: impl Fn(Drag) -> Message + 'a) -> Self {
        self.on_drag = Some(Box::new(f));
        self
    }
    /// A drop target while `active`: `over` when the pointer comes over it, `left` when it goes.
    pub fn drop_target(mut self, active: bool, over: Message, left: Message) -> Self {
        self.drop = active.then_some((over, left));
        self
    }
}

#[derive(Default)]
struct PointerState {
    /// The pointer in the window: the events carry it (inside a scrolling list the cursor is
    /// shifted by the scrolling, a menu wants the real spot).
    at: Option<Point>,
    pressed: Option<Point>,
    dragging: bool,
    over: bool,
}

impl<'a, Message: Clone> Widget<Message, Theme, iced::Renderer> for Pointer<'a, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<PointerState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(PointerState::default())
    }
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
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<PointerState>();
        let at = |state: &PointerState| state.at.or(cursor.position()).unwrap_or(Point::ORIGIN);
        match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                state.at = Some(*position);
                if let (Some(start), Some(on_drag)) = (state.pressed, &self.on_drag) {
                    if state.dragging {
                        shell.publish(on_drag(Drag::Move(*position)));
                    } else if start.distance(*position) > DRAG_START {
                        state.dragging = true;
                        shell.publish(on_drag(Drag::Start(*position)));
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) if cursor.is_over(bounds) => {
                if let Some(on_right) = &self.on_right {
                    shell.publish(on_right(at(state)));
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if cursor.is_over(bounds) => {
                if let Some(on_left) = &self.on_left {
                    shell.publish(on_left(at(state)));
                    shell.capture_event();
                    return;
                }
                if self.on_drag.is_some() {
                    state.pressed = Some(at(state));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.pressed = None;
                if std::mem::take(&mut state.dragging) {
                    if let Some(on_drag) = &self.on_drag {
                        shell.publish(on_drag(Drag::End));
                    }
                    // The button under it lets go without a click.
                    self.content.as_widget_mut().update(&mut tree.children[0], event, layout, mouse::Cursor::Unavailable, renderer, clipboard, shell, viewport);
                    shell.capture_event();
                    return;
                }
            }
            _ => {}
        }
        // A drop target follows the pointer while something is dragged.
        match &self.drop {
            Some((over, left)) => {
                let now = cursor.is_over(bounds);
                if now != state.over {
                    state.over = now;
                    shell.publish(if now { over.clone() } else { left.clone() });
                }
            }
            None => state.over = false,
        }
        self.content.as_widget_mut().update(&mut tree.children[0], event, layout, cursor, renderer, clipboard, shell, viewport);
    }
    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &iced::Renderer) -> mouse::Interaction {
        if tree.state.downcast_ref::<PointerState>().dragging {
            return mouse::Interaction::Grabbing;
        }
        self.content.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }
    fn draw(&self, tree: &Tree, renderer: &mut iced::Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) {
        self.content.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }
    fn overlay<'b>(&'b mut self, tree: &'b mut Tree, layout: Layout<'b>, renderer: &iced::Renderer, viewport: &Rectangle, translation: Vector) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(&mut tree.children[0], layout, renderer, viewport, translation)
    }
}

impl<'a, Message: Clone + 'a> From<Pointer<'a, Message>> for Element<'a, Message> {
    fn from(w: Pointer<'a, Message>) -> Self {
        Element::new(w)
    }
}

/// An invisible layer over everything under a menu: a press anywhere closes the menu. A left
/// press stops there (it only closes); a right press goes on, so right-clicking another card
/// opens its menu at once. Scrolling closes the menu too.
pub struct Dismiss<Message> {
    on_dismiss: Message,
}

pub fn dismiss<Message>(on_dismiss: Message) -> Dismiss<Message> {
    Dismiss { on_dismiss }
}

impl<Message: Clone> Widget<Message, Theme, iced::Renderer> for Dismiss<Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }
    fn layout(&mut self, _tree: &mut Tree, _renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.max())
    }
    fn update(&mut self, _tree: &mut Tree, event: &Event, _layout: Layout<'_>, _cursor: mouse::Cursor, _renderer: &iced::Renderer, _clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, _viewport: &Rectangle) {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                shell.publish(self.on_dismiss.clone());
                if *button != mouse::Button::Right {
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { .. }) => shell.publish(self.on_dismiss.clone()),
            _ => {}
        }
    }
    fn draw(&self, _tree: &Tree, _renderer: &mut iced::Renderer, _theme: &Theme, _style: &renderer::Style, _layout: Layout<'_>, _cursor: mouse::Cursor, _viewport: &Rectangle) {}
}

impl<'a, Message: Clone + 'a> From<Dismiss<Message>> for Element<'a, Message> {
    fn from(w: Dismiss<Message>) -> Self {
        Element::new(w)
    }
}

/// A pane of the split: a press or a scroll in it reports `on_touch` before its content gets the
/// event (and lets the content have it too), so the pane used last becomes the active one.
pub struct Touched<'a, Message> {
    content: Element<'a, Message>,
    on_touch: Message,
}

pub fn touched<'a, Message>(content: impl Into<Element<'a, Message>>, on_touch: Message) -> Touched<'a, Message> {
    Touched { content: content.into(), on_touch }
}

impl<'a, Message: Clone> Widget<Message, Theme, iced::Renderer> for Touched<'a, Message> {
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
        let touch = matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_) | mouse::Event::WheelScrolled { .. }));
        if touch && !shell.is_event_captured() && cursor.is_over(layout.bounds()) {
            shell.publish(self.on_touch.clone());
        }
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

impl<'a, Message: Clone + 'a> From<Touched<'a, Message>> for Element<'a, Message> {
    fn from(w: Touched<'a, Message>) -> Self {
        Element::new(w)
    }
}

/// The split's divider, laid over the whole room: the strip at the cut (`at`, a share of the
/// room) is dragged, reported as the share under the pointer, and let go there. Elsewhere the
/// pointer goes through to the panes.
pub struct Divider<'a, Message> {
    /// Side by side: the strip stands upright.
    across: bool,
    at: f32,
    on_drag: Box<dyn Fn(f32) -> Message + 'a>,
    on_release: Box<dyn Fn(f32) -> Message + 'a>,
}

pub fn divider<'a, Message>(across: bool, at: f32, on_drag: impl Fn(f32) -> Message + 'a, on_release: impl Fn(f32) -> Message + 'a) -> Divider<'a, Message> {
    Divider { across, at, on_drag: Box::new(on_drag), on_release: Box::new(on_release) }
}

#[derive(Default)]
struct DividerState {
    /// Held: the share it was dragged to last.
    held: Option<f32>,
}

/// How far either side of the cut the pointer grabs the divider (px).
const GRAB: f32 = 7.0;

impl<Message> Divider<'_, Message> {
    /// Where the cut is (window px, along the room's long side).
    fn cut(&self, b: Rectangle) -> f32 {
        if self.across { b.x + (b.width * self.at).floor() } else { b.y + (b.height * self.at).floor() }
    }
    fn over(&self, b: Rectangle, p: Point) -> bool {
        let cut = self.cut(b);
        b.contains(p) && (if self.across { p.x } else { p.y } - cut).abs() <= GRAB
    }
    fn share(&self, b: Rectangle, p: Point) -> f32 {
        let s = if self.across { (p.x - b.x) / b.width } else { (p.y - b.y) / b.height };
        s.clamp(0.0, 1.0)
    }
}

impl<'a, Message> Widget<Message, Theme, iced::Renderer> for Divider<'a, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<DividerState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(DividerState::default())
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }
    fn layout(&mut self, _tree: &mut Tree, _renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.max())
    }
    fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, _renderer: &iced::Renderer, _clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, _viewport: &Rectangle) {
        let b = layout.bounds();
        let state = tree.state.downcast_mut::<DividerState>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(p) = cursor.position_over(b).filter(|p| self.over(b, *p)) {
                    state.held = Some(self.share(b, p));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.held.is_some() => {
                let s = self.share(b, *position);
                state.held = Some(s);
                shell.publish((self.on_drag)(s));
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(s) = state.held.take() {
                    shell.publish((self.on_release)(s));
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }
    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, _viewport: &Rectangle, _renderer: &iced::Renderer) -> mouse::Interaction {
        let held = tree.state.downcast_ref::<DividerState>().held.is_some();
        if held || cursor.position_over(layout.bounds()).is_some_and(|p| self.over(layout.bounds(), p)) {
            if self.across { mouse::Interaction::ResizingHorizontally } else { mouse::Interaction::ResizingVertically }
        } else {
            mouse::Interaction::None
        }
    }
    fn draw(&self, tree: &Tree, renderer: &mut iced::Renderer, _theme: &Theme, _style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, _viewport: &Rectangle) {
        let b = layout.bounds();
        let cut = self.cut(b);
        let hot = tree.state.downcast_ref::<DividerState>().held.is_some() || cursor.position_over(b).is_some_and(|p| self.over(b, p));
        let gap = crate::split::GAP;
        // The gap between the panes, and a short handle in its middle.
        let (strip, handle) = if self.across {
            (Rectangle::new(Point::new(cut - gap / 2.0, b.y), Size::new(gap, b.height)), Rectangle::new(Point::new(cut - 2.0, b.y + b.height / 2.0 - 24.0), Size::new(4.0, 48.0)))
        } else {
            (Rectangle::new(Point::new(b.x, cut - gap / 2.0), Size::new(b.width, gap)), Rectangle::new(Point::new(b.x + b.width / 2.0 - 24.0, cut - 2.0), Size::new(48.0, 4.0)))
        };
        renderer.fill_quad(renderer::Quad { bounds: strip, ..Default::default() }, theme::p().chrome);
        renderer.fill_quad(
            renderer::Quad { bounds: handle, border: Border { radius: 2.0.into(), ..Default::default() }, ..Default::default() },
            if hot { theme::p().accent } else { theme::p().muted },
        );
    }
}

impl<'a, Message: 'a> From<Divider<'a, Message>> for Element<'a, Message> {
    fn from(w: Divider<'a, Message>) -> Self {
        Element::new(w)
    }
}
