use iced::{
    advanced::{self, graphics::core::event::Status, layout, widget, Widget},
    Element,
};
use iced_wgpu::primitive::Renderer as PrimitiveRenderer;
use nebula_core::video::state::VideoState;
use std::{marker::PhantomData, time::Duration, time::Instant};

pub struct VideoPlayer<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: PrimitiveRenderer,
{
    video: &'a VideoState,
    content_fit: iced::ContentFit,
    width: iced::Length,
    height: iced::Length,
    on_end_of_frame: Option<Message>,
    on_new_frame: Option<Message>,
    _phantom: PhantomData<(Theme, Renderer)>,
}

impl<'a, Message, Theme, Renderer> VideoPlayer<'a, Message, Theme, Renderer>
where
    Renderer: PrimitiveRenderer,
{
    pub fn new(video: &'a VideoState) -> Self {
        VideoPlayer {
            video,
            content_fit: iced::ContentFit::default(),
            width: iced::Length::Shrink,
            height: iced::Length::Shrink,
            on_end_of_frame: None,
            on_new_frame: None,
            _phantom: Default::default(),
        }
    }

    pub fn width(self, width: impl Into<iced::Length>) -> Self {
        VideoPlayer {
            width: width.into(),
            ..self
        }
    }

    pub fn height(self, height: impl Into<iced::Length>) -> Self {
        VideoPlayer {
            height: height.into(),
            ..self
        }
    }

    pub fn content_fit(self, content_fit: iced::ContentFit) -> Self {
        VideoPlayer {
            content_fit,
            ..self
        }
    }

    pub fn on_end_of_frame(self, message: Message) -> Self {
        VideoPlayer {
            on_end_of_frame: Some(message),
            ..self
        }
    }

    pub fn on_new_frame(self, message: Message) -> Self {
        VideoPlayer {
            on_new_frame: Some(message),
            ..self
        }
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for VideoPlayer<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: PrimitiveRenderer,
{
    fn size(&self) -> iced::Size<iced::Length> {
        iced::Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let (video_width, video_height) = (
            self.video.decoder.width() as f32,
            self.video.decoder.height() as f32,
        );

        let image_size = iced::Size::new(video_width, video_height);
        let raw_size = limits.resolve(self.width, self.height, image_size);
        let full_size = self.content_fit.fit(image_size, raw_size);
        let final_size = iced::Size {
            width: match self.width {
                iced::Length::Shrink => f32::min(raw_size.width, full_size.width),
                _ => raw_size.width,
            },
            height: match self.height {
                iced::Length::Shrink => f32::min(raw_size.height, full_size.height),
                _ => raw_size.height,
            },
        };

        layout::Node::new(final_size)
    }

    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &advanced::renderer::Style,
        layout: advanced::Layout<'_>,
        _cursor: advanced::mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
        let bounds = layout.bounds();
        let image_size = iced::Size::new(
            self.video.decoder.width() as f32,
            self.video.decoder.height() as f32,
        );

        let adjusted_fit = self.content_fit.fit(image_size, bounds.size());
        let scale = iced::Vector::new(
            adjusted_fit.width / image_size.width,
            adjusted_fit.height / image_size.height,
        );
        let final_size = image_size * scale;

        let position = match self.content_fit {
            iced::ContentFit::None => iced::Point::new(
                bounds.x + (image_size.width - adjusted_fit.width) / 2.0,
                bounds.y + (image_size.height - adjusted_fit.height) / 2.0,
            ),
            _ => iced::Point::new(
                bounds.center_x() - final_size.width / 2.0,
                bounds.center_y() - final_size.height / 2.0,
            ),
        };

        let drawing_bounds = iced::Rectangle::new(position, final_size);

        // // Update and render video frame
        // if let Err(e) = self.video.render() {
        //     tracing::error!("Failed to render video frame: {:?}", e);
        // }

        // Draw using your VideoRenderer's primitive
        let render = |renderer: &mut Renderer| {
            renderer.draw_primitive(drawing_bounds, self.video.primitive.clone());
        };

        if adjusted_fit.width > bounds.width || adjusted_fit.height > bounds.height {
            renderer.with_layer(bounds, render);
        } else {
            render(renderer);
        }
    }

    fn on_event(
        &mut self,
        _state: &mut widget::Tree,
        event: iced::Event,
        _layout: advanced::Layout<'_>,
        _cursor: advanced::mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn advanced::Clipboard,
        shell: &mut advanced::Shell<'_, Message>,
        _viewport: &iced::Rectangle,
    ) -> Status {
        if let iced::Event::Window(iced::window::Event::RedrawRequested(_)) = event {
            if self.video.is_playing() {
                // Check if we've reached the end
                if self.video.current_frame() >= self.video.end_frame() {
                    if let Some(ref message) = self.on_end_of_frame {
                        shell.publish(message.clone());
                    }
                }

                if let Some(ref message) = self.on_new_frame {
                    shell.publish(message.clone());
                }

                shell.request_redraw(iced::window::RedrawRequest::NextFrame);
            } else {
                shell.request_redraw(iced::window::RedrawRequest::At(
                    Instant::now() + Duration::from_millis(32),
                ));
            }
            Status::Captured
        } else {
            Status::Ignored
        }
    }
}

impl<'a, Message, Theme, Renderer> From<VideoPlayer<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + PrimitiveRenderer,
{
    fn from(video_player: VideoPlayer<'a, Message, Theme, Renderer>) -> Self {
        Self::new(video_player)
    }
}
