use iced::{
    widget::slider::{Handle, HandleShape, Rail, Status, Style},
    Border, Color, Theme,
};

const ACCENT_YELLOW: Color = Color::from_rgb(1.0, 0.8, 0.0);
const ACCENT_YELLOW_DARK: Color = Color::from_rgb(0.8, 0.6, 0.0);
const TEXT_LIGHT: Color = Color::from_rgb(0.9, 0.9, 0.9);
const TEXT_DARK: Color = Color::BLACK;

pub fn comparison_slider_style(_theme: &Theme, _status: Status) -> Style {
    Style {
        rail: Rail {
            backgrounds: (
                iced::Background::Color(ACCENT_YELLOW).scale_alpha(0.),
                iced::Background::Color(ACCENT_YELLOW_DARK).scale_alpha(0.),
            ),
            width: 4.0,
            border: Border {
                radius: 2.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
        },
        handle: Handle {
            shape: HandleShape::Circle { radius: 10.0 },
            background: iced::Background::Color(ACCENT_YELLOW),
            border_width: 1.0,
            border_color: TEXT_LIGHT,
        },
    }
}
