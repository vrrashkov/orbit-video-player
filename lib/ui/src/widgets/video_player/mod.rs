use iced_wgpu::primitive::Renderer as PrimitiveRenderer;
use nebula_core::video::stream::VideoStream;
use std::{cell::RefCell, marker::PhantomData};

pub mod compariosn_slider;
pub mod element;
pub mod icons;
pub mod theme;

pub struct Video<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: PrimitiveRenderer,
{
    video: &'a RefCell<VideoStream>,
    content_fit: iced::ContentFit,
    width: iced::Length,
    height: iced::Length,
    on_end_of_stream: Option<Message>,
    on_new_frame: Option<Message>,
    comparison_enabled: bool,
    comparison_position: f32,
    dragging_comparison: bool,
    on_comparison_drag_start: Option<Message>,
    on_comparison_drag_end: Option<Message>,
    on_comparison_position_change: Option<Message>,
    _phantom: PhantomData<(Theme, Renderer)>,
}

impl<'a, Message, Theme, Renderer> Video<'a, Message, Theme, Renderer>
where
    Renderer: PrimitiveRenderer,
{
    pub fn new(video: &'a RefCell<VideoStream>) -> Self {
        Video {
            video,
            content_fit: iced::ContentFit::default(),
            width: iced::Length::Shrink,
            height: iced::Length::Shrink,
            on_end_of_stream: None,
            on_new_frame: None,
            comparison_enabled: false,
            comparison_position: 0.5,
            dragging_comparison: false,
            _phantom: Default::default(),
            on_comparison_drag_start: None,
            on_comparison_drag_end: None,
            on_comparison_position_change: None,
        }
    }
    pub fn comparison_enabled(mut self, enabled: bool) -> Self {
        self.comparison_enabled = enabled;
        self
    }

    pub fn comparison_position(mut self, position: f32) -> Self {
        self.comparison_position = position;
        self
    }

    pub fn on_comparison_drag_start(mut self, message: Message) -> Self {
        self.on_comparison_drag_start = Some(message);
        self
    }

    pub fn on_comparison_drag_end(mut self, message: Message) -> Self {
        self.on_comparison_drag_end = Some(message);
        self
    }

    pub fn on_comparison_position_change(mut self, message: Message) -> Self {
        self.on_comparison_position_change = Some(message);
        self
    }
    pub fn width(self, width: impl Into<iced::Length>) -> Self {
        Video {
            width: width.into(),
            ..self
        }
    }

    pub fn height(self, height: impl Into<iced::Length>) -> Self {
        Video {
            height: height.into(),
            ..self
        }
    }

    pub fn content_fit(self, content_fit: iced::ContentFit) -> Self {
        Video {
            content_fit,
            ..self
        }
    }

    pub fn on_end_of_stream(self, message: Message) -> Self {
        Video {
            on_end_of_stream: Some(message),
            ..self
        }
    }

    pub fn on_new_frame(self, message: Message) -> Self {
        Video {
            on_new_frame: Some(message),
            ..self
        }
    }
}
