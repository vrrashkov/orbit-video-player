use iced_wgpu::primitive::Renderer as PrimitiveRenderer;
use nebula_core::video::stream::VideoStream;
use std::{cell::RefCell, marker::PhantomData};

pub mod element;
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
            _phantom: Default::default(),
        }
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
