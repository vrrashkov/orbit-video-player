use ffmpeg_next as ffmpeg;
use iced::{
    widget::{Button, Column, Container, Row, Slider, Text},
    Element,
};
use iced::{Renderer, Settings};
use nebula_common::VideoError;
use nebula_core::video::decoder::VideoDecoder;
use nebula_ui::components::player::VideoPlayer;
use std::{cell::RefCell, time::Duration};
use std::{path::Path, sync::Arc};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use winit::{event_loop::EventLoop, window::WindowBuilder};

fn main() -> iced::Result {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    iced::run("Iced Video Player", App::update, App::view)
}

#[derive(Clone, Debug)]
enum Message {
    TogglePause,
    ToggleLoop,
    Seek(f64),
    SeekRelease,
    EndOfStream,
    NewFrame,
}
struct App {
    video: RefCell<VideoDecoder>,
    position: f64,
    dragging: bool,
}

impl Default for App {
    fn default() -> Self {
        let video_path = "assets/videos/video1.mp4";
        if !Path::new(video_path).exists() {
            panic!("Video file not found at: {}", video_path);
        }
        let start_frame = 1;
        let end_frame = 435;
        let state = RefCell::new(VideoDecoder::new(video_path, start_frame, end_frame).unwrap());

        App {
            video: state,
            position: 0.0,
            dragging: false,
        }
    }
}

impl App {
    fn update(&mut self, message: Message) {
        match message {
            Message::TogglePause => {
                if !self.video.borrow().is_playing {
                    self.video.borrow_mut().play();
                } else {
                    self.video.borrow_mut().pause();
                }
            }
            Message::ToggleLoop => {}
            Message::Seek(_) => {}
            Message::SeekRelease => {}
            Message::EndOfStream => {}
            Message::NewFrame => {
                if !self.dragging {
                    let current = self.video.borrow().current_frame();
                    tracing::info!("Current frame: {}", current);
                    self.position = current as f64;
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let is_playing = self.video.borrow().is_playing;
        let total_frames = self.video.borrow().total_frames();
        let is_looping = self.video.borrow().looping();

        Column::new()
            .push(
                Container::new(
                    VideoPlayer::new(&self.video)
                        .width(iced::Length::Fill)
                        .height(iced::Length::Fill)
                        .content_fit(iced::ContentFit::Contain)
                        // .on_end_of_stream(Message::EndOfStream)
                        .on_new_frame(Message::NewFrame),
                )
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::Center)
                .width(iced::Length::Fill)
                .height(iced::Length::Fill),
            )
            .push(
                Container::new(
                    Slider::new(0.0..=total_frames as f64, self.position, Message::Seek)
                        .step(0.1)
                        .on_release(Message::SeekRelease),
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
                            .on_press(Message::TogglePause),
                    )
                    .push(
                        Button::new(Text::new(if is_looping {
                            "Disable Loop"
                        } else {
                            "Enable Loop"
                        }))
                        .width(120.0)
                        .on_press(Message::ToggleLoop),
                    )
                    .push(
                        Text::new(format!(
                            "{}:{:02}s",
                            self.position as u64 / 60,
                            self.position as u64 % 60,
                            // self.video.total_frames().as_secs() / 60,
                            // self.video.total_frames().as_secs() % 60,
                        ))
                        .width(iced::Length::Fill)
                        .align_x(iced::alignment::Horizontal::Right),
                    ),
            )
            .into()
    }
}
