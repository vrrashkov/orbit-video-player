use iced::alignment::Vertical;
use iced::widget::{container, svg};
use iced::{Element, Length};

pub fn create<'a, Message: 'a>(
    name: &str,
    width: f32,
    height: f32,
    color: Option<iced::Color>,
) -> Element<'a, Message> {
    let handle = svg::Handle::from_path(format!("assets/icons/{}.svg", name));

    let svg_element = svg(handle)
        .width(Length::Fixed(width))
        .height(Length::Fixed(height));

    // Apply color if provided
    let svg_with_color = if let Some(color) = color {
        svg_element.style(move |_theme, _status| svg::Style { color: Some(color) })
    } else {
        svg_element
    };

    // Center vertically in a container
    container(svg_with_color).align_y(Vertical::Center).into()
}

pub fn play<'a, Message: 'a>(size: f32, color: Option<iced::Color>) -> Element<'a, Message> {
    create("play", size, size, color)
}

pub fn pause<'a, Message: 'a>(size: f32, color: Option<iced::Color>) -> Element<'a, Message> {
    create("pause", size, size, color)
}

pub fn comparison<'a, Message: 'a>(size: f32, color: Option<iced::Color>) -> Element<'a, Message> {
    create("comparison", size, size, color)
}
