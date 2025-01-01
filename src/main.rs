use nebula_ui::components::player::VideoPlayer;

fn main() -> iced::Result {
    iced::run("Video Player", (), App::view)
}

struct App {
    video: Video,
}

impl Default for App {
    fn default() -> Self {
        App {
            video: Video::new(&url::Url::parse("file:///C:/my_video.mp4").unwrap()).unwrap(),
        }
    }
}

impl App {
    fn view(&self) -> iced::Element<()> {
        VideoPlayer::new(&mut &self.video).into()
    }
}
