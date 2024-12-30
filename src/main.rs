use ui::Counter;

fn main() -> iced::Result {
    iced::run("A cool counter", Counter::update, Counter::view)
}
