use super::{AdvancedNetworkSettings, Context, WifiManagerUi};
use crate::{
    assets::Texture,
    draw::Draw,
    geometry::Rect,
    label::gui_label,
    text::Font,
    text_layout::{Horizontal, TextStyle, Vertical},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::Cell, rc::Rc};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Panel {
    #[default]
    Wifi,
    Advanced,
}
pub struct NavButton {
    pub state: WidgetState,
    pub text: String,
}
impl NavButton {
    pub fn new(text: String) -> Self {
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 100.0,
        };
        Self { state, text }
    }
}
impl Widget for NavButton {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let color = if self.state.is_pressed() {
            [74, 74, 74, 255]
        } else {
            [57, 57, 57, 255]
        };
        draw.rounded_segments(self.state.rect, 0.6, 10, u32::from_le_bytes(color), false)?;
        gui_label(
            draw,
            self.state.rect,
            &self.text,
            TextStyle {
                font: Font::Normal,
                size: 60.0,
                spacing: 0.0,
                color: u32::from_le_bytes([255, 255, 255, 229]),
            },
            (Horizontal::Center, Vertical::Middle),
            true,
        )?;
        Ok(RenderResult::None)
    }
}
pub struct NetworkUi {
    pub state: WidgetState,
    pub wifi: WifiManagerUi,
    pub advanced: AdvancedNetworkSettings,
    pub panel: Rc<Cell<Panel>>,
    navigation: NavButton,
    context: Context,
}
impl NetworkUi {
    pub fn new(
        context: Context,
        params: Rc<openpilot_params::Params>,
        mut texture: impl FnMut(&str, (i32, i32)) -> Result<Texture, Error>,
    ) -> Result<Self, Error> {
        let wifi = WifiManagerUi::new(context.clone(), &mut texture)?;
        let advanced = AdvancedNetworkSettings::new(context.clone(), params, &mut texture)?;
        let panel = Rc::new(Cell::new(Panel::Wifi));
        let mut navigation = NavButton::new(context.text("Advanced"));
        let current = panel.clone();
        navigation.state.click = Some(Box::new(move || {
            current.set(match current.get() {
                Panel::Wifi => Panel::Advanced,
                Panel::Advanced => Panel::Wifi,
            })
        }));
        Ok(Self {
            state: WidgetState::default(),
            wifi,
            advanced,
            panel,
            navigation,
            context,
        })
    }
}
impl Widget for NetworkUi {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.wifi.show(frame);
        self.advanced.show(frame);
        self.navigation.show(frame);
        self.panel.set(Panel::Wifi);
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.wifi.hide(frame);
        self.advanced.hide(frame);
        self.navigation.hide(frame);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let content = Rect {
            y: rect.y + 140.0,
            height: rect.height - 140.0,
            ..rect
        };
        match self.panel.get() {
            Panel::Wifi => {
                self.navigation.text = self.context.text("Advanced");
                self.navigation.set_position(
                    rect.x + rect.width - self.navigation.state.rect.width,
                    rect.y + 20.0,
                );
                self.wifi.set_rect(content);
                self.wifi.render(frame, draw)?;
            }
            Panel::Advanced => {
                self.navigation.text = self.context.text("Back");
                self.navigation.set_position(rect.x, rect.y + 20.0);
                self.advanced.set_rect(content);
                self.advanced.render(frame, draw)?;
            }
        }
        self.navigation.render(frame, draw)?;
        Ok(RenderResult::None)
    }
}
