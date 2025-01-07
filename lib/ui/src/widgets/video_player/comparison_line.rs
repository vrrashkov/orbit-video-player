use iced::{
    advanced::{
        self, graphics::core::event::Status, layout, mouse, widget, Clipboard, Shell, Widget,
    },
    border::Radius,
    event,
    widget::container,
    Background, Border, Element, Event, Shadow, Theme,
};
use iced::{
    advanced::{renderer, widget::Tree, Layout},
    Length, Point, Rectangle, Size, Vector,
};
use iced_wgpu::primitive::Renderer as PrimitiveRenderer;
use nebula_core::video::{primitive::VideoPrimitive, stream::VideoStream};
use std::{
    cell::RefCell,
    marker::PhantomData,
    time::{Duration, Instant},
};

use iced::widget::{Button, Column, Container, Row, Slider, Text};

pub struct ComparisonLine<Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: PrimitiveRenderer,
{
    position: f32,
    width: f32,
    on_drag: Option<Message>,
    _phantom: PhantomData<(Theme, Renderer)>,
}

#[derive(Debug, Clone)]
pub enum ComparisonLineEvent {
    DraggedTo(f32),
}

impl<Message, Theme, Renderer> ComparisonLine<Message, Theme, Renderer>
where
    Renderer: PrimitiveRenderer,
{
    pub fn new(position: f32) -> Self {
        Self {
            position,
            width: 4.0, // Line width in pixels
            on_drag: None,
            _phantom: Default::default(),
        }
    }
    pub fn update(&mut self, message: ComparisonLineEvent) {
        match message {
            ComparisonLineEvent::DraggedTo(value) => {
                self.position = value;
            }
        }
    }
    pub fn on_drag(mut self, message: Message) -> Self {
        self.on_drag = Some(message);
        self
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for ComparisonLine<Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: PrimitiveRenderer,
{
    fn size(&self) -> iced::Size<iced::Length> {
        iced::Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn layout(
        &self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _state: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let x = bounds.x + (bounds.width * self.position);

        let line = renderer::Quad {
            bounds: Rectangle {
                x: x - self.width / 2.0,
                y: bounds.y,
                width: self.width,
                height: bounds.height,
            },
            // background: renderer::Background::Color([1.0, 1.0, 1.0, 0.8].into()),
            border: Border {
                color: Default::default(),
                width: 0.0,
                radius: Radius::default(),
            },
            shadow: Shadow::default(),
        };

        renderer.fill_quad(line, Background::Color([1.0, 1.0, 1.0, 0.8].into()));
    }

    fn on_event(
        &mut self,
        _state: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> event::Status {
        let bounds = layout.bounds();
        let line_x = bounds.x + (bounds.width * self.position);

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position() {
                    if (position.x - line_x).abs() < 10.0 {
                        // Start dragging
                        shell.publish(Message::DraggedTo(self.position));
                        return event::Status::Captured;
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let new_position = ((position.x - bounds.x) / bounds.width).clamp(0.0, 1.0);
                if let Some(ref on_drag) = self.on_drag {
                    shell.publish((on_drag)(new_position));
                }
                return event::Status::Captured;
            }
            _ => {}
        }

        event::Status::Ignored
    }
}
