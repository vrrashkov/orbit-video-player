use iced::widget::{column, horizontal_space, Checkbox};
use iced::{
    advanced::{self, graphics::core::event::Status, layout, widget, Widget},
    widget::Stack,
    Alignment::Center,
    Element, Length,
};
use iced_wgpu::primitive::Renderer as PrimitiveRenderer;
use nebula_core::video::{primitive::VideoPrimitive, stream::VideoStream};
use std::collections::HashMap;
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

use iced::widget::{Button, Column, Container, Row, Slider, Text};

use super::icons::{comparison, pause, play};
use super::theme::{
    controls_container, primary_button, secondary_button, text_time, video_container, video_slider,
};
use super::{compariosn_slider::comparison_slider_style, Video};

pub struct Player {
    stream: RefCell<VideoStream>,
    position: f64,
    dragging: bool,
    // Comparison
    comparison_enabled: bool,
    comparison_position: f32,
    dragging_comparison: bool,
    // Shader selections
    shader_selections: HashMap<String, bool>,
}

#[derive(Clone, Debug)]
pub enum Event {
    Pause,
    Loop,
    Seek(f64),
    SeekRelease,
    EndOfStream,
    NewFrame,
    // Comparison
    ToggleComparison,
    UpdateComparisonPosition(f32),
    ComparisonDragStart,
    ComparisonDragEnd,
    // New event for shader selection
    ToggleShader(String, bool),
}

impl Player {
    pub fn new(stream: RefCell<VideoStream>, position: f64, dragging: bool) -> Self {
        let mut shader_selections = HashMap::new();

        // Default shader selections
        shader_selections.insert("upscale".to_string(), true);

        Self {
            stream,
            position,
            dragging,
            // Comparison
            comparison_enabled: false,
            comparison_position: 0.5, // Start at middle
            dragging_comparison: false,
            shader_selections,
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
            Event::ToggleShader(name, enabled) => {
                self.shader_selections.insert(name, enabled);
            }
            Event::Loop => {
                self.stream.borrow_mut().looping();
            }
            Event::Seek(secs) => {
                self.dragging = true;
                self.stream.borrow_mut().pause(); // Pause while seeking

                self.position = secs;
                let seek_result = self.stream.borrow_mut().seek_to_time(self.position);
                match seek_result {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("Failed to seek: {:?}", e)
                    }
                }
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
                    // Only update if the difference is significant
                    if (self.position - current).abs() > 0.001 {
                        self.position = current;
                    }
                }
            }
            // Comparison
            Event::ToggleComparison => {
                self.comparison_enabled = !self.comparison_enabled;
            }
            Event::UpdateComparisonPosition(pos) => {
                self.comparison_position = pos.clamp(0.0, 1.0);
            }
            Event::ComparisonDragStart => {
                self.dragging_comparison = true;
            }
            Event::ComparisonDragEnd => {
                self.dragging_comparison = false;
            }
        }
    }

    pub fn view(&self) -> Element<Event> {
        let is_playing = self.stream.borrow().is_playing;
        let _is_looping = self.stream.borrow().looping();
        let current = self.stream.borrow().current_time();
        let total = self.stream.borrow().total_time().unwrap();
        let shader_controls = Container::new(
            Column::new()
                .spacing(10)
                .push(Text::new("Active Shaders:").style(text_time))
                .push(
                    Row::new().spacing(10).push(
                        Checkbox::new(
                            "Upscale",
                            *self.shader_selections.get("upscale").unwrap_or(&false),
                        )
                        .on_toggle(|enabled| Event::ToggleShader("upscale".to_string(), enabled)),
                    ),
                ),
        )
        .padding(10)
        .style(controls_container);
        let video_row = {
            let mut row = Stack::new().push(
                Container::new(
                    Video::new(&self.stream)
                        .width(iced::Length::Fill)
                        .height(iced::Length::Fill)
                        .content_fit(iced::ContentFit::Contain)
                        .comparison_enabled(self.comparison_enabled)
                        .comparison_position(self.comparison_position)
                        .shader_selections(self.shader_selections.clone())
                        .on_comparison_drag_start(Event::ComparisonDragStart)
                        .on_comparison_drag_end(Event::ComparisonDragEnd)
                        .on_comparison_position_change(Event::UpdateComparisonPosition(
                            self.comparison_position,
                        ))
                        .on_end_of_stream(Event::EndOfStream)
                        .on_new_frame(Event::NewFrame),
                )
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .style(video_container),
            );

            if self.comparison_enabled {
                row = row.push(
                    Container::new(
                        Slider::new(0.0..=1.0, self.comparison_position, |pos| {
                            Event::UpdateComparisonPosition(pos)
                        })
                        .style(comparison_slider_style)
                        .step(0.001),
                    )
                    .width(iced::Length::Fill)
                    .height(iced::Length::Fill)
                    .align_y(Center)
                    .align_x(Center),
                );
            }

            row
        };

        Column::new()
            .push(
                Container::new(column![video_row])
                    .width(iced::Length::Fill)
                    .height(iced::Length::Fill)
                    .style(video_container),
            )
            .push(
                Container::new(
                    Slider::new(0.0..=total.as_secs_f64(), self.position, Event::Seek)
                        .step(0.001)
                        .on_release(Event::SeekRelease)
                        .style(video_slider),
                )
                .padding(iced::Padding::new(15.0).left(15.0).right(15.0))
                .style(controls_container),
            )
            .push(
                Container::new(
                    Row::new()
                        .spacing(10)
                        .align_y(iced::alignment::Vertical::Center)
                        .padding(iced::Padding::new(10.0))
                        .push(
                            Button::new(
                                Row::new()
                                    .spacing(5)
                                    .align_y(iced::alignment::Alignment::Center)
                                    .push(if !is_playing {
                                        play(16.0, None)
                                    } else {
                                        pause(16.0, None)
                                    })
                                    .push(Text::new(if !is_playing { "Play" } else { "Pause" })),
                            )
                            .width(100.0)
                            .on_press(Event::Pause)
                            .style(primary_button),
                        )
                        .push(
                            Button::new(
                                Row::new()
                                    .spacing(5)
                                    .align_y(iced::alignment::Alignment::Center)
                                    .push(comparison(16.0, None))
                                    .push(Text::new(if self.comparison_enabled {
                                        "Disable Comparison"
                                    } else {
                                        "Enable Comparison"
                                    })),
                            )
                            .width(180.0)
                            .on_press(Event::ToggleComparison)
                            .style(secondary_button),
                        )
                        .push(horizontal_space())
                        .push(
                            Text::new(format!(
                                "{:02}:{:02} / {:02}:{:02}",
                                (current.as_secs_f64() / 60.0).floor() as u64,
                                (current.as_secs_f64() % 60.0).floor() as u64,
                                (total.as_secs_f64() / 60.0).floor() as u64,
                                (total.as_secs_f64() % 60.0).floor() as u64
                            ))
                            .style(text_time),
                        ),
                )
                .style(controls_container)
                .width(Length::Fill),
            )
            .push(shader_controls)
            .spacing(1)
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

        // Get frame data, whether playing or not
        let frame_data = if let Ok(Some(data)) = video.update() {
            Some(data)
        } else {
            None
        };

        // Render frame if we have data
        if let Some(frame_data) = frame_data {
            let frame_id = video.current_frame();
            // tracing::info!("Rendering frame {}", frame_id);

            let primitive = VideoPrimitive::new(
                frame_id, // Use current frame as unique ID
                true,     // Force update
                frame_data,
                (image_size.width as _, image_size.height as _),
                true, // Always create new texture
                video.color_space,
            )
            .with_comparison(self.comparison_enabled)
            .with_comparison_position(self.comparison_position)
            .with_shader_selections(self.shader_selections.clone());

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
        layout: advanced::Layout<'_>,
        cursor: advanced::mouse::Cursor,
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
                if video.current_frame() >= video.end_frame().unwrap() {
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
        } else if let iced::Event::Mouse(mouse_event) = event {
            let bounds = layout.bounds();

            if self.comparison_enabled {
                let split_x = bounds.x + (bounds.width * self.comparison_position);

                match mouse_event {
                    iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left) => {
                        if let Some(position) = cursor.position() {
                            if (position.x - split_x).abs() < 10.0 {
                                if let Some(ref message) = self.on_comparison_drag_start {
                                    shell.publish(message.clone());
                                }
                                return Status::Captured;
                            }
                        }
                    }
                    iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left) => {
                        if let Some(ref message) = self.on_comparison_drag_end {
                            shell.publish(message.clone());
                        }
                        return Status::Captured;
                    }
                    iced::mouse::Event::CursorMoved { position } => {
                        if self.dragging_comparison {
                            let bounds = layout.bounds();
                            let new_position =
                                ((position.x - bounds.x) / bounds.width).clamp(0.0, 1.0);

                            self.comparison_position = new_position;

                            if let Some(ref message) = self.on_comparison_position_change {
                                shell.publish(message.clone());
                            }
                            return Status::Captured;
                        }
                    }
                    _ => {}
                }
            }

            return Status::Ignored;
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
