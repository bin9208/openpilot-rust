use crate::{
    assets::Texture,
    button::{Button, ButtonStyle},
    callback::Callback,
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    html::HtmlRenderer,
    label::gui_label,
    text::Font,
    text_layout::{self as text, float, Horizontal, TextStyle, Vertical},
    toggle::Toggle,
    widget::{Frame, Property, RenderResult, Widget, WidgetState},
    Error,
};
use num_traits::ToPrimitive;
use std::{cell::Cell, rc::Rc};
const VALUE_COLOR: u32 = u32::from_le_bytes([170, 170, 170, 255]);
fn text_style(color: u32) -> TextStyle {
    TextStyle {
        font: Font::Normal,
        size: 50.0,
        spacing: 0.0,
        color,
    }
}
fn valid(state: &WidgetState) -> bool {
    state.interaction_gate && state.touch_valid.as_ref().is_none_or(|callback| callback())
}
pub trait ItemAction: Widget {
    fn width_hint(&self, draw: &dyn Draw) -> f64;
}

mod actions;
mod buttons;
pub use actions::{ButtonAction, TextAction, ToggleAction};
pub use buttons::{DualButtonAction, MultipleButtonAction};
pub struct ListItem {
    pub state: WidgetState,
    pub title: Property<String>,
    pub description: Property<String>,
    pub description_visible: bool,
    pub icon: Option<Texture>,
    pub action: Option<Box<dyn ItemAction>>,
    pub callback: Option<Callback<()>>,
    pub description_opened: Option<Callback<()>>,
    html: HtmlRenderer,
    previous_description: String,
}
impl ListItem {
    pub fn new(title: impl Into<String>) -> Result<Self, Error> {
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 600.0,
            height: 170.0,
        };
        let mut html = HtmlRenderer::new("", 40.0)?;
        html.color = u32::from_le_bytes([128, 128, 128, 255]);
        Ok(Self {
            state,
            title: title.into().into(),
            description: String::new().into(),
            description_visible: false,
            icon: None,
            action: None,
            callback: None,
            description_opened: None,
            html,
            previous_description: String::new(),
        })
    }
    pub fn action_mut<T: ItemAction>(&mut self) -> Option<&mut T> {
        self.action
            .as_mut()
            .and_then(|action| (action.as_mut() as &mut dyn std::any::Any).downcast_mut::<T>())
    }
    pub fn right_rect(&self, draw: &dyn Draw) -> Rect {
        let Some(action) = &self.action else {
            return Rect::default();
        };
        let rect = self.state.rect;
        let width = action.width_hint(draw);
        if width == 0.0 {
            return Rect {
                x: rect.x + 20.0,
                y: rect.y,
                width: rect.width - 40.0,
                height: 170.0,
            };
        }
        let title = text::measure(draw, Font::Normal, &self.title.get(), 50.0, 0.0).x;
        let width = width.min(f64::from(rect.width) - 40.0 - f64::from(title));
        Rect {
            x: float(f64::from(rect.x + rect.width) - width),
            y: rect.y,
            width: float(width),
            height: 170.0,
        }
    }
    pub fn height(&mut self, draw: &dyn Draw, width: f64) -> f64 {
        if !self.state.visible.get() {
            return 0.0;
        }
        if self.description_visible {
            170.0 + self.html.total_height(draw, width) - 30.0 + 20.0
        } else {
            170.0
        }
    }
    fn update_description(&mut self) -> Result<(), Error> {
        let value = self.description.get();
        if value != self.previous_description {
            self.html.parse(&value)?;
            self.previous_description = value;
        }
        Ok(())
    }
    pub fn set_description_visible(&mut self, value: bool, draw: &dyn Draw) -> Result<(), Error> {
        if !self.description.get().is_empty() && self.description_visible != value {
            self.description_visible = value;
            if value {
                if let Some(callback) = &self.description_opened {
                    callback.call(());
                    self.update_description()?;
                }
            }
            self.state.rect.height =
                float(self.height(draw, (f64::from(self.state.rect.width) - 40.0).trunc()));
        }
        Ok(())
    }
}
impl Widget for ListItem {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn set_parent_rect(&mut self, rect: Rect) {
        self.state.parent_rect = Some(rect);
        self.state.rect.width = rect.width;
    }
    fn show(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.show(frame);
        }
        if !self.description.get().is_empty() && self.description_visible {
            self.description_visible = false;
            self.state.rect.height = 170.0;
        }
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.update_description()
    }
    fn mouse_release(
        &mut self,
        position: Point,
        _: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        if !self.state.visible.get() {
            return Ok(());
        }
        if self.action.is_some() && self.right_rect(draw).contains(position) {
            return Ok(());
        }
        self.set_description_visible(!self.description_visible, draw)
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        if self.state.parent_rect.is_some_and(|parent| {
            rect.y + rect.height <= parent.y || rect.y >= parent.y + parent.height
        }) {
            return Ok(RenderResult::None);
        }
        let title = self.title.get();
        if !title.is_empty() {
            let mut x = rect.x + 20.0;
            if let Some(icon) = self.icon {
                icon.draw(
                    draw,
                    Point {
                        x,
                        y: rect.y + (170.0 - icon.height) / 2.0,
                    },
                    1.0,
                    WHITE,
                )?;
                x += 100.0;
            }
            let size = text::measure(draw, Font::Normal, &title, 50.0, 0.0);
            text::draw_text(
                draw,
                Font::Normal,
                &title,
                Point {
                    x,
                    y: rect.y + ((170.0 - size.y) / 2.0).floor(),
                },
                50.0,
                0.0,
                WHITE,
            )?;
        }
        if self.description_visible {
            let width = (f64::from(rect.width) - 40.0).trunc();
            let height = self.html.total_height(draw, width);
            self.html.set_rect(Rect {
                x: rect.x + 20.0,
                y: rect.y + 140.0,
                width: float(width),
                height: float(height),
            });
            self.html.render(frame, draw)?;
        }
        let right = self.right_rect(draw);
        let gate = valid(&self.state);
        if let Some(action) = &mut self.action {
            action.set_rect(right);
            action.state_mut().interaction_gate = gate;
            let result = action.render(frame, draw)?;
            if result.truthy() && action.state().enabled.get() {
                if let Some(callback) = &self.callback {
                    callback.call(());
                }
            }
        }
        Ok(RenderResult::None)
    }
}
