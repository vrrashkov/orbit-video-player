use iced::{
    advanced::{self, graphics::core::event::Status, layout, widget, Widget},
    Element,
};
use iced_wgpu::primitive::Renderer as PrimitiveRenderer;
use nebula_core::video::{primitive::VideoPrimitive, stream::VideoStream};
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

use iced::widget::{Button, Column, Container, Row, Slider, Text};

use super::Video;

pub struct Player {
    stream: RefCell<VideoStream>,
    position: f64,
    dragging: bool,
}

#[derive(Clone, Debug)]
pub enum Event {
    Pause,
    Loop,
    Seek(f64),
    SeekRelease,
    EndOfStream,
    NewFrame,
}

impl Player {
    pub fn new(stream: RefCell<VideoStream>, position: f64, dragging: bool) -> Self {
        Self {
            stream,
            position,
            dragging,
        }
    }
    pub fn update(&mut self, message: Event) {
        match message {
            Event::Pause => {
                if !self.stream.borrow().is_playing {
                    self.stream.borrow_mut().play();
                } else {
                    self.stream.borrow_mut().pause();
                }
            }
            Event::Loop => {
                self.stream.borrow_mut().looping();
            }
            Event::Seek(secs) => {
                self.dragging = true;
                self.stream.borrow_mut().pause();
                self.position = secs;
                let seek_result = self.stream.borrow_mut().seek_to_time(self.position);
                match seek_result {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("Failed to seek: {:?}", e)
                    }
                }
                let current = self.stream.borrow().current_time().as_secs_f64();
                tracing::info!("Current time: {}", current);
            }
            Event::SeekRelease => {
                self.dragging = false;
                self.stream.borrow_mut().pause();
            }
            Event::EndOfStream => {
                self.stream.borrow_mut().pause();
            }
            Event::NewFrame => {
                if !self.dragging {
                    let current = self.stream.borrow().current_time().as_secs_f64();
                    tracing::info!("Current frame: {}", current);
                    self.position = current;
                }
            }
        }
    }

    pub fn view(&self) -> Element<Event> {
        let is_playing = self.stream.borrow().is_playing;
        let total_frames = self.stream.borrow().total_frames();
        let is_looping = self.stream.borrow().looping();
        let current = self.stream.borrow().current_time();
        let total = self.stream.borrow().total_time();

        Column::new()
            .push(
                Container::new(
                    Video::new(&self.stream)
                        .width(iced::Length::Fill)
                        .height(iced::Length::Fill)
                        .content_fit(iced::ContentFit::Contain)
                        .on_end_of_stream(Event::EndOfStream)
                        .on_new_frame(Event::NewFrame),
                )
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::Center)
                .width(iced::Length::Fill)
                .height(iced::Length::Fill),
            )
            .push(
                Container::new(
                    Slider::new(0.0..=total.as_secs_f64(), self.position, Event::Seek)
                        .step(0.1)
                        .on_release(Event::SeekRelease),
                )
                .padding(iced::Padding::new(5.0).left(10.0).right(10.0)),
            )
            .push(
                Row::new()
                    .spacing(5)
                    .align_y(iced::alignment::Vertical::Center)
                    .padding(iced::Padding::new(10.0).top(0.0))
                    .push(
                        Button::new(Text::new(if !is_playing { "Play" } else { "Pause" }))
                            .width(80.0)
                            .on_press(Event::Pause),
                    )
                    .push(
                        Button::new(Text::new(if is_looping {
                            "Disable Loop"
                        } else {
                            "Enable Loop"
                        }))
                        .width(120.0)
                        .on_press(Event::Loop),
                    )
                    .push(
                        Text::new(format!(
                            "{:02}:{:02} / {:02}:{:02}",
                            current.as_secs() / 60,
                            current.as_secs() % 60,
                            total.as_secs() / 60,
                            total.as_secs() % 60,
                        ))
                        .width(iced::Length::Fill)
                        .align_x(iced::alignment::Horizontal::Right),
                    ),
            )
            .into()
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Video<'a, Message, Theme, Renderer>
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
            self.video.borrow().decoder.width() as f32,
            self.video.borrow().decoder.height() as f32,
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
        let mut video = self.video.borrow_mut();
        let bounds = layout.bounds();
        let image_size =
            iced::Size::new(video.decoder.width() as f32, video.decoder.height() as f32);

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
        if video.is_playing() {
            tracing::info!("Video playing, requesting frame");
            if let Ok(Some(frame_data)) = video.update() {
                // Call update() here
                tracing::info!("Got frame data at frame {}", video.current_frame());
                let primitive = VideoPrimitive::new(
                    1,
                    true,
                    frame_data,
                    (image_size.width as _, image_size.height as _),
                    true,
                );

                let render = |renderer: &mut Renderer| {
                    renderer.draw_primitive(drawing_bounds, primitive.clone());
                };

                if adjusted_fit.width > bounds.width || adjusted_fit.height > bounds.height {
                    renderer.with_layer(bounds, render);
                } else {
                    render(renderer);
                }
            }
        } else if let Some(last_frame) = video.get_last_frame() {
            // Render the last frame if we're not getting a new one
            let primitive = VideoPrimitive::new(
                1,
                true,
                last_frame,
                (image_size.width as _, image_size.height as _),
                // Show texture when frame changing
                true,
            );

            let render = |renderer: &mut Renderer| {
                renderer.draw_primitive(drawing_bounds, primitive.clone());
            };

            if adjusted_fit.width > bounds.width || adjusted_fit.height > bounds.height {
                renderer.with_layer(bounds, render);
            } else {
                render(renderer);
            }
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
            let video = self.video.borrow_mut();

            if video.is_playing() {
                // Get the video's frame duration
                let frame_duration = video.get_frame_duration();
                shell.request_redraw(iced::window::RedrawRequest::NextFrame);

                if let Some(ref message) = self.on_new_frame {
                    shell.publish(message.clone());
                }
                // Check for end of video
                if video.current_frame() >= video.end_frame() {
                    if let Some(ref message) = self.on_end_of_stream {
                        shell.publish(message.clone());
                    }
                }

                // Only schedule one redraw
                shell.request_redraw(iced::window::RedrawRequest::At(
                    Instant::now() + frame_duration,
                ));
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

impl<'a, Message, Theme, Renderer> From<Video<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + PrimitiveRenderer,
{
    fn from(video_player: Video<'a, Message, Theme, Renderer>) -> Self {
        Self::new(video_player)
    }
}
