use crate::{
    animation::Filter,
    assets::Texture,
    draw::{Draw, BLACK, WHITE},
    geometry::Point,
    label::Label,
    text::Font,
    text_layout::{float, Horizontal},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use num_traits::ToPrimitive;
const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> u32 {
    u32::from_le_bytes([red, green, blue, alpha])
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonStyle {
    #[default]
    Normal,
    Primary,
    Danger,
    Transparent,
    TransparentWhiteText,
    TransparentWhiteBorder,
    Action,
    ListAction,
    NoEffect,
    Keyboard,
    ForgetWifi,
}
impl ButtonStyle {
    pub fn text(self) -> u32 {
        match self {
            Self::Transparent | Self::Action => BLACK,
            Self::TransparentWhiteText => WHITE,
            Self::Keyboard => rgba(221, 221, 221, 255),
            Self::ForgetWifi => rgba(51, 51, 51, 255),
            _ => rgba(228, 228, 228, 255),
        }
    }
    pub fn background(self, pressed: bool) -> u32 {
        match (self, pressed) {
            (Self::Normal | Self::NoEffect, false)
            | (Self::NoEffect, true)
            | (Self::Keyboard, true) => rgba(51, 51, 51, 255),
            (Self::Normal, true) => rgba(74, 74, 74, 255),
            (Self::Primary, false) => rgba(70, 91, 234, 255),
            (Self::Primary, true) => rgba(48, 73, 244, 255),
            (Self::Danger, false) => rgba(226, 44, 44, 255),
            (Self::Danger, true) => rgba(255, 36, 36, 255),
            (Self::Transparent, _) | (Self::TransparentWhiteBorder, false) => BLACK,
            (Self::TransparentWhiteText, _) | (Self::TransparentWhiteBorder, true) => 0,
            (Self::Action | Self::ForgetWifi, false) => rgba(189, 189, 189, 255),
            (Self::Action | Self::ForgetWifi, true) => rgba(130, 130, 130, 255),
            (Self::ListAction, false) => rgba(57, 57, 57, 255),
            (Self::ListAction, true) => rgba(74, 74, 74, 74),
            (Self::Keyboard, false) => rgba(68, 68, 68, 255),
        }
    }
}
pub struct Button {
    pub state: WidgetState,
    pub label: Label,
    pub style: ButtonStyle,
    pub radius: f64,
    pub selected: Option<bool>,
    pub selection_icon: Option<Texture>,
    background: u32,
}
impl Button {
    pub fn new(text: impl Into<String>) -> Self {
        let mut label = Label::new(text);
        label.font = Font::Medium;
        label.padding = 20.0;
        label.color = ButtonStyle::Normal.text();
        Self {
            state: WidgetState::default(),
            label,
            style: ButtonStyle::Normal,
            radius: 10.0,
            selected: None,
            selection_icon: None,
            background: ButtonStyle::Normal.background(false),
        }
    }
    pub fn radio(text: impl Into<String>, icon: Option<Texture>) -> Self {
        let mut button = Self::new(text);
        button.label.horizontal = Horizontal::Left;
        button.selected = Some(false);
        button.selection_icon = icon;
        button
    }
    pub fn set_style(&mut self, style: ButtonStyle) {
        self.style = style;
        self.background = style.background(false);
        self.label.color = style.text();
    }
}
impl Widget for Button {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, _draw: &mut dyn Draw) -> Result<(), Error> {
        if let Some(selected) = self.selected {
            self.background = if selected {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Normal
            }
            .background(false);
            return Ok(());
        }
        if self.state.enabled.get() {
            self.label.color = self.style.text();
            self.background = self.style.background(self.state.is_pressed());
        } else if self.style != ButtonStyle::NoEffect {
            self.background = if self.style == ButtonStyle::TransparentWhiteText {
                0
            } else {
                rgba(51, 51, 51, 255)
            };
            self.label.color = if self.style == ButtonStyle::TransparentWhiteText {
                WHITE
            } else {
                rgba(228, 228, 228, 51)
            };
        }

        Ok(())
    }
    fn mouse_release(
        &mut self,
        _: Point,
        frame: &Frame<'_>,
        _draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.state.release(frame.now);
        if let Some(selected) = &mut self.selected {
            *selected = !*selected;
        }

        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let roundness = float(self.radius / (f64::from(rect.width.min(rect.height)) / 2.0));
        if self.selected.is_none() && self.style == ButtonStyle::TransparentWhiteBorder {
            draw.rounded(rect, roundness, BLACK)?;
            draw.border(rect, roundness, WHITE)?;
        } else {
            draw.rounded(rect, roundness, self.background)?;
        }
        self.label.set_rect(rect);
        self.label.render(frame, draw)?;
        if self.selected == Some(true) {
            if let Some(icon) = self.selection_icon {
                icon.draw(
                    draw,
                    Point {
                        x: rect.x + rect.width - icon.width - float(self.label.padding) - 15.0,
                        y: rect.y + (rect.height - icon.height) / 2.0,
                    },
                    1.0,
                    if self.state.enabled.get() {
                        WHITE
                    } else {
                        rgba(255, 255, 255, 100)
                    },
                )?;
            }
        }
        Ok(RenderResult::None)
    }
}

mod icons;
pub use icons::{IconButton, SmallCircleIconButton};
