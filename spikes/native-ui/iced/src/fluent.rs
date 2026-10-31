//! The Fluent design tokens of `src/lib/brand.ts` and the Fluent neutrals, as an iced theme and
//! a handful of style functions. This is the layer a native rewrite would have to own, since
//! iced has no CSS and no ready-made Fluent set. Colour values follow Fluent 2 (web), radii
//! are the `borderRadiusMedium` (4) and `borderRadiusLarge` (8) tokens.

use iced::theme::Palette;
use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

pub const RADIUS_MEDIUM: f32 = 4.0;
pub const RADIUS_LARGE: f32 = 8.0;

const BRAND_HUE: f32 = 218.0;
const BRAND_SATURATION: f32 = 88.0;
const DARKEST: f32 = 7.0;
const LIGHTEST: f32 = 94.0;

fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn hsl(hue: f32, saturation: f32, lightness: f32) -> Color {
    let s = saturation / 100.0;
    let l = lightness / 100.0;
    let a = s * l.min(1.0 - l);
    let channel = |n: f32| {
        let k = (n + hue / 30.0) % 12.0;
        l - a * (-1.0f32).max((k - 3.0).min((9.0 - k).min(1.0)))
    };
    Color::from_rgb(channel(0.0), channel(8.0), channel(4.0))
}

/// The brand ramp of `brand.ts`: step 10 is the darkest, 160 the lightest.
pub fn brand(step: u32) -> Color {
    let index = (step / 10 - 1) as f32;
    let lightness = DARKEST + index * (LIGHTEST - DARKEST) / 15.0;
    let edge = index.min(15.0 - index);
    let saturation = BRAND_SATURATION - (3.0 - edge).max(0.0) * 8.0;
    hsl(BRAND_HUE, saturation, lightness)
}

/// The neutral and brand colours one style function needs, picked by the theme's darkness.
#[derive(Clone, Copy)]
pub struct Tokens {
    pub background1: Color,
    pub background2: Color,
    pub background3: Color,
    /// A card: the foreground at 4 % over the window.
    pub card: Color,
    pub card_hover: Color,
    pub foreground1: Color,
    pub foreground2: Color,
    pub foreground3: Color,
    pub stroke1: Color,
    pub stroke2: Color,
    pub brand: Color,
    pub brand_hover: Color,
    pub brand_pressed: Color,
    pub on_brand: Color,
}

impl Tokens {
    pub fn of(theme: &Theme) -> Self {
        if theme.palette().background.r < 0.5 {
            Self::dark()
        } else {
            Self::light()
        }
    }

    pub fn dark() -> Self {
        Self {
            background1: rgb(0x29_29_29),
            background2: rgb(0x1F_1F_1F),
            background3: rgb(0x14_14_14),
            card: Color::from_rgba(1.0, 1.0, 1.0, 0.04),
            card_hover: Color::from_rgba(1.0, 1.0, 1.0, 0.08),
            foreground1: Color::WHITE,
            foreground2: rgb(0xD6_D6_D6),
            foreground3: rgb(0xAD_AD_AD),
            stroke1: rgb(0x66_66_66),
            stroke2: rgb(0x52_52_52),
            brand: brand(70),
            brand_hover: brand(80),
            brand_pressed: brand(40),
            on_brand: Color::WHITE,
        }
    }

    pub fn light() -> Self {
        Self {
            background1: Color::WHITE,
            background2: rgb(0xFA_FA_FA),
            background3: rgb(0xF5_F5_F5),
            card: Color::from_rgba(0.0, 0.0, 0.0, 0.04),
            card_hover: Color::from_rgba(0.0, 0.0, 0.0, 0.08),
            foreground1: rgb(0x24_24_24),
            foreground2: rgb(0x42_42_42),
            foreground3: rgb(0x61_61_61),
            stroke1: rgb(0xD1_D1_D1),
            stroke2: rgb(0xE0_E0_E0),
            brand: brand(80),
            brand_hover: brand(70),
            brand_pressed: brand(40),
            on_brand: Color::WHITE,
        }
    }
}

/// The iced theme for either scheme: Fluent's window background and text, the brand colour as
/// primary. Widgets that are not given a style function use these five colours.
pub fn theme(dark: bool) -> Theme {
    let t = if dark { Tokens::dark() } else { Tokens::light() };
    Theme::custom(
        if dark { "Aether dark" } else { "Aether light" }.to_owned(),
        Palette {
            background: t.background2,
            text: t.foreground1,
            primary: t.brand,
            success: rgb(0x10_7C_10),
            warning: rgb(0xF7_63_0C),
            danger: rgb(0xC5_0F_1F),
        },
    )
}

fn border(color: Color, radius: f32) -> Border {
    Border {
        color,
        width: 1.0,
        radius: radius.into(),
    }
}

/// A card: a 4 % overlay with a hairline and large corners.
pub fn card(theme: &Theme) -> container::Style {
    let t = Tokens::of(theme);
    container::Style {
        background: Some(Background::Color(t.card)),
        border: border(t.stroke2, RADIUS_LARGE),
        text_color: Some(t.foreground1),
        ..container::Style::default()
    }
}

/// The page layer: the lighter surface with rounded top corners, as in `shell.css`.
pub fn page(theme: &Theme) -> container::Style {
    let t = Tokens::of(theme);
    container::Style {
        background: Some(Background::Color(t.background1)),
        border: Border {
            color: t.stroke2,
            width: 1.0,
            radius: iced::border::top(RADIUS_LARGE),
        },
        text_color: Some(t.foreground1),
        ..container::Style::default()
    }
}

/// The primary action (like "登录微软账户"): brand fill, hover and pressed steps.
pub fn primary_button(theme: &Theme, status: button::Status) -> button::Style {
    let t = Tokens::of(theme);
    let fill = match status {
        button::Status::Active => t.brand,
        button::Status::Hovered => t.brand_hover,
        button::Status::Pressed => t.brand_pressed,
        button::Status::Disabled => t.stroke2,
    };
    button::Style {
        background: Some(Background::Color(fill)),
        text_color: if matches!(status, button::Status::Disabled) {
            t.foreground3
        } else {
            t.on_brand
        },
        border: Border {
            radius: RADIUS_MEDIUM.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

/// The secondary action: outlined, neutral fill.
pub fn secondary_button(theme: &Theme, status: button::Status) -> button::Style {
    let t = Tokens::of(theme);
    let fill = match status {
        button::Status::Active => t.background1,
        button::Status::Hovered => t.background3,
        button::Status::Pressed => t.background2,
        button::Status::Disabled => t.background1,
    };
    button::Style {
        background: Some(Background::Color(fill)),
        text_color: if matches!(status, button::Status::Disabled) {
            t.foreground3
        } else {
            t.foreground1
        },
        border: border(t.stroke1, RADIUS_MEDIUM),
        ..button::Style::default()
    }
}

/// A list row or navigation item: transparent until hovered; the selected one is a card with
/// a brand bar, like the account cards.
pub fn nav_item(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = Tokens::of(theme);
        let fill = match (selected, status) {
            (true, _) => t.card_hover,
            (false, button::Status::Hovered | button::Status::Pressed) => t.card,
            _ => Color::TRANSPARENT,
        };
        button::Style {
            background: Some(Background::Color(fill)),
            text_color: if selected { t.foreground1 } else { t.foreground2 },
            border: Border {
                color: if selected { t.brand } else { Color::TRANSPARENT },
                width: if selected { 1.0 } else { 0.0 },
                radius: RADIUS_MEDIUM.into(),
            },
            shadow: Shadow {
                color: Color::TRANSPARENT,
                offset: Vector::ZERO,
                blur_radius: 0.0,
            },
            ..button::Style::default()
        }
    }
}

/// A text box: neutral fill and hairline, the brand colour on focus.
pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let t = Tokens::of(theme);
    let line = match status {
        text_input::Status::Active => t.stroke1,
        text_input::Status::Hovered => t.foreground3,
        text_input::Status::Focused { .. } => t.brand,
        text_input::Status::Disabled => t.stroke2,
    };
    text_input::Style {
        background: Background::Color(t.background1),
        border: border(line, RADIUS_MEDIUM),
        icon: t.foreground3,
        placeholder: t.foreground3,
        value: t.foreground1,
        selection: Color { a: 0.35, ..t.brand },
    }
}
