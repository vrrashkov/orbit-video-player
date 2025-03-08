use iced::widget::{button, container, slider, text};
use iced::{Border, Color, Shadow, Theme};

// Define custom colors - black and yellow bee theme
const BACKGROUND: Color = Color::from_rgb(0.1, 0.1, 0.1);
const SURFACE: Color = Color::from_rgb(0.15, 0.15, 0.15);
const ACCENT_YELLOW: Color = Color::from_rgb(1.0, 0.8, 0.0);
const ACCENT_YELLOW_DARK: Color = Color::from_rgb(0.8, 0.6, 0.0);
pub const TEXT_LIGHT: Color = Color::from_rgb(0.9, 0.9, 0.9);
const TEXT_DARK: Color = Color::BLACK;

pub fn text_style(_theme: &Theme) -> text::Style {
    text::Style {
        color: Some(TEXT_LIGHT),
    }
}

pub fn text_yellow(_theme: &Theme) -> text::Style {
    text::Style {
        color: Some(ACCENT_YELLOW),
    }
}

pub fn text_time(_theme: &Theme) -> text::Style {
    text::Style {
        color: Some(Color::from_rgb(0.7, 0.7, 0.7)),
    }
}
pub fn primary_button(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Active => button::Style {
            background: Some(ACCENT_YELLOW.into()),
            text_color: TEXT_DARK,
            border: Border {
                radius: 4.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            shadow: Shadow {
                color: Color {
                    a: 0.2,
                    ..Color::BLACK
                },
                offset: iced::Vector::new(0.0, 1.0),
                blur_radius: 2.0,
            },
        },
        button::Status::Hovered => button::Style {
            background: Some(
                Color {
                    a: 0.9,
                    ..ACCENT_YELLOW
                }
                .into(),
            ),
            text_color: TEXT_DARK,
            border: Border {
                radius: 4.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            shadow: Shadow {
                color: Color {
                    a: 0.3,
                    ..Color::BLACK
                },
                offset: iced::Vector::new(0.0, 2.0),
                blur_radius: 3.0,
            },
        },
        button::Status::Pressed => button::Style {
            background: Some(ACCENT_YELLOW_DARK.into()),
            text_color: TEXT_DARK,
            border: Border {
                radius: 4.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            shadow: Shadow {
                color: Color {
                    a: 0.1,
                    ..Color::BLACK
                },
                offset: iced::Vector::new(0.0, 0.0),
                blur_radius: 1.0,
            },
        },
        button::Status::Disabled => button::Style {
            background: Some(
                Color {
                    a: 0.5,
                    ..ACCENT_YELLOW
                }
                .into(),
            ),
            text_color: Color {
                a: 0.5,
                ..TEXT_DARK
            },
            border: Border {
                radius: 4.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            shadow: Shadow::default(),
        },
    }
}

pub fn secondary_button(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Active => button::Style {
            background: Some(SURFACE.into()),
            text_color: ACCENT_YELLOW,
            border: Border {
                radius: 4.0.into(),
                width: 1.0,
                color: ACCENT_YELLOW,
            },
            shadow: Shadow {
                color: Color {
                    a: 0.2,
                    ..Color::BLACK
                },
                offset: iced::Vector::new(0.0, 1.0),
                blur_radius: 2.0,
            },
        },
        button::Status::Hovered => button::Style {
            background: Some(
                Color {
                    a: 0.2,
                    ..ACCENT_YELLOW
                }
                .into(),
            ),
            text_color: ACCENT_YELLOW,
            border: Border {
                radius: 4.0.into(),
                width: 1.0,
                color: ACCENT_YELLOW,
            },
            shadow: Shadow {
                color: Color {
                    a: 0.3,
                    ..Color::BLACK
                },
                offset: iced::Vector::new(0.0, 2.0),
                blur_radius: 3.0,
            },
        },
        button::Status::Pressed => button::Style {
            background: Some(
                Color {
                    a: 0.3,
                    ..ACCENT_YELLOW
                }
                .into(),
            ),
            text_color: ACCENT_YELLOW,
            border: Border {
                radius: 4.0.into(),
                width: 1.0,
                color: ACCENT_YELLOW,
            },
            shadow: Shadow {
                color: Color {
                    a: 0.1,
                    ..Color::BLACK
                },
                offset: iced::Vector::new(0.0, 0.0),
                blur_radius: 1.0,
            },
        },
        button::Status::Disabled => button::Style {
            background: Some(SURFACE.into()),
            text_color: Color {
                a: 0.5,
                ..ACCENT_YELLOW
            },
            border: Border {
                radius: 4.0.into(),
                width: 1.0,
                color: Color {
                    a: 0.5,
                    ..ACCENT_YELLOW
                },
            },
            shadow: Shadow::default(),
        },
    }
}

// Slider styles - using the proper Rail and Handle structure
pub fn video_slider(_theme: &Theme, _status: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (ACCENT_YELLOW.into(), SURFACE.into()),
            width: 6.0,
            border: Border {
                radius: 3.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 8.0 },
            background: ACCENT_YELLOW.into(),
            border_width: 1.0,
            border_color: TEXT_DARK,
        },
    }
}

pub fn comparison_slider_style(_theme: &Theme, _status: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (
                ACCENT_YELLOW.into(),
                Color {
                    a: 0.7,
                    ..ACCENT_YELLOW_DARK
                }
                .into(),
            ),
            width: 4.0,
            border: Border {
                radius: 2.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 10.0 },
            background: ACCENT_YELLOW.into(),
            border_width: 1.0,
            border_color: TEXT_LIGHT,
        },
    }
}

// Container styles - using the proper Style structure
pub fn video_container(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(TEXT_LIGHT),
        background: Some(Color::BLACK.into()),
        border: Border {
            radius: 6.0.into(),
            width: 1.0,
            color: ACCENT_YELLOW_DARK,
        },
        shadow: Shadow {
            color: Color {
                a: 0.2,
                ..Color::BLACK
            },
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 5.0,
        },
    }
}

pub fn controls_container(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(TEXT_LIGHT),
        background: Some(BACKGROUND.into()),
        border: Border {
            radius: 6.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
    }
}
