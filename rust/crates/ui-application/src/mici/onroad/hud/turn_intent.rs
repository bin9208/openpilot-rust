use crate::{
    context::Context,
    onroad::model_renderer::math::{byte, float},
    paint::{self, color, Image},
    state::messages,
};
use openpilot_cereal::log_capnp::onroad_event::EventName;
use openpilot_startup_ui::renderer::TextureOptions;
use openpilot_ui_framework::{
    animation::Filter,
    assets::Texture,
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
#[derive(serde::Serialize)]
pub struct Snapshot {
    pub pre: bool,
    pub direction: i32,
    pub alpha: f64,
    pub rotation: f64,
}
pub struct TurnIntent {
    state: WidgetState,
    context: Context,
    pre: bool,
    direction: i32,
    alpha: Filter,
    rotation: Filter,
    left: Texture,
    right: Texture,
}
impl TurnIntent {
    pub fn new(context: Context, canvas: &mut Canvas, fps: f64) -> Result<Self, Error> {
        Ok(Self {
            state: WidgetState::default(),
            context,
            pre: false,
            direction: 0,
            alpha: Filter::new(0.0, 0.05, fps),
            rotation: Filter::new(0.0, 0.1, fps),
            left: paint::texture(canvas, "icons_mici/turn_intent_left.png", (50, 20))?,
            right: canvas.texture(
                "icons_mici/turn_intent_left.png",
                TextureOptions {
                    width: Some(50),
                    height: Some(20),
                    flip_x: true,
                    ..Default::default()
                },
            )?,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            pre: self.pre,
            direction: self.direction,
            alpha: self.alpha.x,
            rotation: self.rotation.x,
        }
    }
}
impl Widget for TurnIntent {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let messages = self.context.messages.borrow();
        let mut left = false;
        let mut right = false;
        let mut change = false;
        for event in messages::onroad_events(&messages.state)? {
            match event.get_name().map_err(crate::Error::from)? {
                EventName::PreLaneChangeLeft => left = true,
                EventName::PreLaneChangeRight => right = true,
                EventName::LaneChange => change = true,
                _ => {}
            }
        }
        if left || right {
            if !self.pre {
                self.rotation.x = if left { 30.0 } else { -30.0 };
            }
            self.pre = true;
            self.direction = if left { -1 } else { 1 };
            self.alpha.update(1.0);
            self.rotation.update(0.0);
        } else if change {
            self.pre = false;
            self.alpha.update(0.0);
            self.rotation.update(f64::from(self.direction * 30));
        } else {
            self.pre = false;
            self.direction = 0;
            self.alpha.update(0.0);
            self.rotation.update(0.0);
        }
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if self.alpha.x > 1e-2 {
            let texture = if self.direction == 1 {
                self.right
            } else {
                self.left
            };
            let rect = self.state.rect;
            paint::image(
                draw,
                Image {
                    texture,
                    rect: Rect {
                        x: float(f64::from(rect.x) + f64::from(rect.width) / 2.0),
                        y: float(f64::from(rect.y) + f64::from(rect.height) / 2.0),
                        width: texture.width,
                        height: texture.height,
                    },
                    tint: color(255, 255, 255, byte(255.0 * self.alpha.x)?),
                    origin: Point {
                        x: texture.width / 2.0,
                        y: rect.height / 2.0,
                    },
                    rotation: float(self.rotation.x),
                },
            )?;
        }
        Ok(RenderResult::None)
    }
}
