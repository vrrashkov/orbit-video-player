use iced::{widget::Container, Element};
use orbit_video_player_core::video::stream::{VideoStream, VideoStreamOptions};
use orbit_video_player_ui::widgets::video_player::element::Player;
use std::cell::RefCell;
use std::path::Path;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

fn main() -> iced::Result {
    env_logger::init();
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    iced::run("Orbit Video Player", App::update, App::view)
}

pub struct App {
    video_player: Player,
}
#[derive(Clone, Debug)]
pub enum Message {
    VideoPlayer(orbit_video_player_ui::widgets::video_player::element::Event),
}

impl Default for App {
    fn default() -> Self {
        let video_path = "assets/videos/test2_resolution.mp4";
        if !Path::new(video_path).exists() {
            panic!("Video file not found at: {}", video_path);
        }
        let start_frame = 1;
        let end_frame = None;
        let stream = RefCell::new(
            VideoStream::new(VideoStreamOptions {
                video_path,
                start_frame,
                end_frame,
            })
            .unwrap(),
        );

        App {
            video_player: Player::new(stream, 0.0, false),
        }
    }
}

impl App {
    fn update(&mut self, message: Message) {
        match message {
            Message::VideoPlayer(msg) => self.video_player.update(msg),
        }
    }

    fn view(&self) -> Element<Message> {
        Container::new(self.video_player.view().map(Message::VideoPlayer)).into()
    }
}
