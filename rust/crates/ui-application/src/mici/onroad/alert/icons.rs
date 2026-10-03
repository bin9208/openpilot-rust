use super::Alerts;
use crate::{
    onroad::alert::Alert,
    paint::{self, color},
    state::messages,
};
use num_traits::ToPrimitive;
use openpilot_startup_ui::renderer::TextureOptions;
use openpilot_ui_framework::{
    assets::Texture,
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    text_layout::float,
    Error,
};
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Side {
    Left,
    Right,
}
pub(super) struct Icon {
    texture: Texture,
    side: Side,
    margin: (f64, f64),
    signal: bool,
}
pub(super) struct Layout {
    pub rect: Rect,
    pub icon: Option<Icon>,
}
impl Layout {
    pub fn left(&self) -> bool {
        self.icon
            .as_ref()
            .is_some_and(|icon| icon.side == Side::Left)
    }
}
pub(super) fn load(canvas: &mut Canvas) -> Result<[Texture; 4], Error> {
    let mut load = |path: &str, size: (i32, i32), flip_x| {
        canvas.texture(
            path,
            TextureOptions {
                width: Some(size.0),
                height: Some(size.1),
                flip_x,
                ..Default::default()
            },
        )
    };
    Ok([
        load("icons_mici/onroad/turn_signal_left.png", (104, 96), false)?,
        load("icons_mici/onroad/turn_signal_left.png", (104, 96), true)?,
        load("icons_mici/onroad/blind_spot_left.png", (134, 150), false)?,
        load("icons_mici/onroad/blind_spot_left.png", (134, 150), true)?,
    ])
}
impl Alerts {
    pub(super) fn layout_alert(&mut self, alert: &Alert) -> Result<Layout, Error> {
        let mut side = None;
        let mut texture = None;
        let mut margin = (20.0, 18.0);
        let mut signal = false;
        match alert.alert_type.split('/').next().unwrap_or_default() {
            "preLaneChangeLeft" => {
                side = Some(Side::Left);
                texture = Some(self.icons[0]);
                margin = (2.0, 5.0);
                signal = true;
            }
            "preLaneChangeRight" => {
                side = Some(Side::Right);
                texture = Some(self.icons[1]);
                margin = (2.0, 5.0);
                signal = true;
            }
            name @ ("laneChange" | "laneChangeBlocked") => {
                let messages = self.context.messages.borrow();
                let car = messages::car_state(&messages.state)?;
                side = if car.get_left_blinker() {
                    Some(Side::Left)
                } else if car.get_right_blinker() {
                    Some(Side::Right)
                } else {
                    self.last_side
                };
                if name == "laneChange" {
                    texture = Some(self.icons[usize::from(self.last_side != Some(Side::Left))]);
                    margin = (2.0, 5.0);
                    signal = true;
                } else {
                    texture = Some(self.icons[2 + usize::from(side != Some(Side::Left))]);
                    margin = (8.0, 0.0);
                }
            }
            _ => self.timer = 0.0,
        }
        self.last_side = side;
        let rect = self.state.rect;
        let x = f64::from(rect.x)
            + if side == Some(Side::Left) {
                f64::from(self.icons[1].width)
            } else {
                18.0
            };
        let width = f64::from(rect.width)
            - 18.0
            - if side.is_some() {
                f64::from(self.icons[1].width)
            } else {
                0.0
            };
        Ok(Layout {
            rect: Rect {
                x: float(x),
                y: float(self.y.position.x),
                width: float(width),
                height: rect.height,
            },
            icon: texture.zip(side).map(|(texture, side)| Icon {
                texture,
                side,
                margin,
                signal,
            }),
        })
    }
    pub(super) fn icon(&mut self, draw: &mut dyn Draw, layout: &Layout) -> Result<(), Error> {
        let Some(icon) = &layout.icon else {
            return Ok(());
        };
        let now = (self.context.now_monotonic)();
        if now - self.timer > 1.0 / (80.0 / 60.0) {
            self.timer = now;
            self.signal_alpha.x = 510.0;
        } else {
            self.signal_alpha.update(51.0);
        }
        let rect = self.state.rect;
        let x = if icon.side == Side::Left {
            f64::from(rect.x) + icon.margin.0
        } else {
            f64::from(rect.x) + f64::from(rect.width)
                - icon.margin.0
                - f64::from(icon.texture.width)
        };
        let alpha = if icon.signal {
            self.signal_alpha.x.min(255.0).trunc()
        } else {
            255.0
        };
        paint::image(
            draw,
            paint::Image {
                texture: icon.texture,
                rect: Rect {
                    x: float(x.trunc()),
                    y: float((f64::from(rect.y) + icon.margin.1).trunc()),
                    width: icon.texture.width,
                    height: icon.texture.height,
                },
                origin: Point::default(),
                rotation: 0.0,
                tint: color(
                    255,
                    255,
                    255,
                    (alpha * self.alpha.x)
                        .to_u8()
                        .ok_or(Error::Contract("invalid alert icon alpha"))?,
                ),
            },
        )
    }
}
