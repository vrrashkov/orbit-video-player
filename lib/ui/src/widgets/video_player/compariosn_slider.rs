use iced::{
    widget::slider::{Handle, HandleShape, Rail, Status, Style},
    Theme,
};

pub fn comparison_slider_style(theme: &Theme, _status: Status) -> Style {
    let palette = theme.extended_palette();

    Style {
        rail: Rail {
            backgrounds: (
                // Thin vertical line with primary color
                iced::Background::Color(palette.primary.base.color.scale_alpha(0.0)),
                iced::Background::Color(palette.primary.base.color.scale_alpha(0.0)),
            ),
            width: 2.0, // Very thin line
            border: Default::default(),
        },
        handle: Handle {
            // Custom SVG-like handle design
            shape: HandleShape::Circle { radius: 10.0 },
            border_width: 2.0,
            border_color: palette.primary.base.color,
            background: iced::Background::Color(palette.background.base.color),
        },
    }
}
