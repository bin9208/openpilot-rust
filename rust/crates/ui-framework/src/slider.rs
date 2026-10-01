use crate::{
    animation::{Bounce, Filter},
    assets::Texture,
    draw::Draw,
    geometry::{MouseEvent, Point, Rect},
    text::Font,
    text_layout::{float, Horizontal, Vertical},
    unified_label::UnifiedLabel,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use num_traits::ToPrimitive;
#[derive(Clone, Copy)]
pub struct SliderAssets {
    pub background: Texture,
    pub circle: Texture,
    pub pressed: Texture,
    pub arrow: Texture,
}
pub struct Slider {
    pub state: WidgetState,
    pub label: UnifiedLabel,
    pub assets: SliderAssets,
    pub on_confirm: Option<Box<dyn FnMut()>>,
    pub shimmer_offset: f64,
    pub opacity: Filter,
    pub position: Filter,
    pub scale: Bounce,
    pub dragging: bool,
    start_x: f64,
    raw_x: f64,
    confirmed_time: f64,
    callback_called: bool,
    press_time: Option<f64>,
    threshold: f64,
}
impl Slider {
    pub fn new(title: impl Into<String>, assets: SliderAssets, fps: f64, big: bool) -> Self {
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 536.0,
            height: if big { 180.0 } else { 115.0 },
        };
        let mut label = UnifiedLabel::new(title);
        label.size = if big { 48.0 } else { 36.0 };
        label.font = if big { Font::Display } else { Font::SemiBold };
        label.color = u32::MAX;
        label.horizontal = Horizontal::Right;
        label.vertical = Vertical::Middle;
        label.line_height = if big { 0.875 } else { 0.9 };
        label.shimmer = true;
        Self {
            state,
            label,
            assets,
            on_confirm: None,
            shimmer_offset: 0.0,
            opacity: Filter::new(1.0, 0.1, fps),
            position: Filter::new(0.0, 0.05, fps),
            scale: Bounce::new(1.0, 0.1, fps, 2.0),
            dragging: false,
            start_x: 0.0,
            raw_x: 0.0,
            confirmed_time: 0.0,
            callback_called: false,
            press_time: None,
            threshold: -268.0,
        }
    }
    pub fn confirmed(&self) -> bool {
        self.confirmed_time > 0.0
    }
    pub fn percentage(&self) -> f64 {
        (-self.position.x
            / f64::from(-self.assets.background.width + self.assets.circle.width).abs())
        .clamp(0.0, 1.0)
    }
    pub fn reset(&mut self, now: f64) {
        self.dragging = false;
        self.press_time = None;
        self.confirmed_time = 0.0;
        self.callback_called = false;
        self.label.reset_shimmer(now, self.shimmer_offset);
    }
    pub fn set_opacity(&mut self, opacity: f64, smooth: bool) {
        if smooth {
            self.opacity.update(opacity);
        } else {
            self.opacity.x = opacity;
        }
    }
}
impl Widget for Slider {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.show(frame);
        }
        self.label.show(frame);
        self.reset(frame.now);
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.hide(frame);
        }
        self.label.hide(frame);
    }
    fn mouse_event(
        &mut self,
        event: MouseEvent,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        if event.pressed {
            let rect = self.state.rect;
            let hit = Rect {
                x: float(
                    f64::from(rect.x + rect.width - self.assets.circle.width) + self.position.x
                        - 16.0,
                ),
                y: rect.y,
                width: self.assets.circle.width + 16.0,
                height: rect.height,
            };
            if hit.contains(event.pos) {
                self.start_x = f64::from(event.pos.x);
                self.dragging = true;
                self.press_time = Some(frame.now);
            }
        } else if event.released {
            if self.position.x < self.threshold {
                self.confirmed_time = frame.now;
            }
            self.dragging = false;
        }
        if self.dragging {
            self.raw_x = f64::from(event.pos.x) - self.start_x;
        }
        Ok(())
    }
    fn update(&mut self, frame: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let activated = f64::from(-self.assets.background.width + self.assets.circle.width).trunc();
        self.raw_x = self.raw_x.min(0.0).max(activated);
        if self.confirmed() {
            self.position.update(activated);
            if self.position.x < activated + 1.0
                && !self.callback_called
                && frame.now - self.confirmed_time >= 0.2
            {
                self.callback_called = true;
                if let Some(callback) = &mut self.on_confirm {
                    callback();
                }
            }
        } else if !self.dragging {
            self.position.update(0.0);
        } else {
            self.position.x = self.raw_x;
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let alpha = (255.0 * self.opacity.x)
            .to_u8()
            .ok_or(Error::Contract("slider opacity out of range"))?;
        let white = u32::from_le_bytes([255, 255, 255, alpha]);
        let rect = self.state.rect;
        let background = Point {
            x: rect.x + (rect.width - self.assets.background.width) / 2.0,
            y: rect.y + (rect.height - self.assets.background.height) / 2.0,
        };
        self.assets.background.draw(draw, background, 1.0, white)?;
        let button_x =
            f64::from(background.x + self.assets.background.width - self.assets.circle.width)
                + self.position.x;
        let button_y = f64::from(rect.y)
            + (f64::from(rect.height) - f64::from(self.assets.circle.height)) / 2.0;
        let label_alpha = (255.0 * (1.0 - self.percentage()) * self.opacity.x)
            .to_u8()
            .ok_or(Error::Contract("slider label opacity out of range"))?;
        if label_alpha > 0 {
            self.label.color = u32::from_le_bytes([255, 255, 255, label_alpha]);
            self.label.set_rect(Rect {
                x: rect.x + 20.0,
                y: rect.y,
                width: rect.width - self.assets.circle.width - 50.0,
                height: rect.height,
            });
            self.label.render(frame, draw)?;
        }
        let pressed = self.dragging
            || self.confirmed()
            || self.press_time.is_some_and(|time| frame.now - time < 0.075);
        let circle = if pressed {
            self.assets.pressed
        } else {
            self.assets.circle
        };
        let scale = self.scale.update(if pressed { 1.07 } else { 1.0 });
        let x = button_x + f64::from(self.assets.circle.width) * (1.0 - scale) / 2.0;
        let y = button_y + f64::from(self.assets.circle.height) * (1.0 - scale) / 2.0;
        circle.draw(
            draw,
            Point {
                x: float(x),
                y: float(y),
            },
            float(scale),
            white,
        )?;
        self.assets.arrow.draw(
            draw,
            Point {
                x: float(
                    button_x + f64::from(self.assets.circle.width - self.assets.arrow.width) / 2.0,
                ),
                y: float(y + f64::from(self.assets.circle.height - self.assets.arrow.height) / 2.0),
            },
            1.0,
            white,
        )?;
        Ok(RenderResult::None)
    }
}
#[cfg(feature = "native")]
mod assets;
