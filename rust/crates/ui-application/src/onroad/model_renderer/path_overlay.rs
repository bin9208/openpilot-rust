use super::{
    drawing::{self, Anchor, BoxStyle, Label},
    math::{float, integer},
    points::ScreenPoint,
    ModelRenderer,
};
use crate::{paint::color, Error};
use openpilot_ui_framework::{draw::Draw, geometry::Rect};
impl ModelRenderer {
    pub(super) fn draw_path_end(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        let c = &self.carrot;
        let x = f64::from(c.path_x);
        let y = f64::from(c.path_y) + 60.;
        let mut distance = false;
        let label = if c.soft_hold != 0 || c.brake_hold || c.cruise != 0 {
            Some(if c.brake_hold {
                "AUTOHOLD"
            } else if c.soft_hold != 0 {
                "SOFTHOLD"
            } else {
                "CARROT"
            })
        } else if c.long_active {
            match c.x_state {
                3 | 5 => Some(if c.speed < 1. {
                    if c.traffic_state >= 1000 {
                        "Signal Error"
                    } else {
                        "Signal Ready"
                    }
                } else {
                    "Signal slowing"
                }),
                4 => Some("E2E주행중"),
                0..=2 => {
                    distance = true;
                    None
                }
                _ => None,
            }
        } else {
            distance = true;
            None
        };
        if let Some(label) = label {
            drawing::text(draw, Label::center(label, ScreenPoint([x, y]), 50.))?;
        }
        if distance {
            let tint = match c.x_state {
                0 => color(255, 255, 255, 255),
                1 => color(191, 191, 191, 255),
                _ => color(0, 203, 0, 255),
            };
            if c.radar_distance > 0. {
                let value = format!("{:.1}", c.radar_distance);
                let position = ScreenPoint([x - 80., y]);
                let bg = if c.track_id < 1 {
                    color(255, 0, 0, 255)
                } else {
                    color(255, 175, 3, 255)
                };
                drawing::text_box(draw, Label::center(&value, position, 40.), bg)?;
                drawing::text(
                    draw,
                    Label {
                        color: tint,
                        ..Label::center(&value, position, 40.)
                    },
                )?;
            }
            if c.vision_distance > 0. {
                let value = format!("{:.1}", c.vision_distance);
                let position = ScreenPoint([x + 80., y]);
                drawing::text_box(
                    draw,
                    Label::center(&value, position, 40.),
                    color(0, 0, 255, 255),
                )?;
                drawing::text(
                    draw,
                    Label {
                        color: tint,
                        ..Label::center(&value, position, 40.)
                    },
                )?;
            }
        }
        if c.follow_distance > 0. {
            if let (Some(left), Some(right)) = (c.follow_left, c.follow_right) {
                draw.line(
                    drawing::point(left),
                    drawing::point(right),
                    3.,
                    color(255, 255, 255, 255),
                )?;
                let value = format!("{:.0} m", c.follow_distance);
                let position = ScreenPoint([
                    f64::from(integer(right.0[0])?) + 10.,
                    f64::from(integer(right.0[1])?),
                ]);
                drawing::text(
                    draw,
                    Label {
                        anchor: Anchor::LeftTop,
                        ..Label::center(&value, position, 25.)
                    },
                )?;
            }
        }
        if c.lead_status {
            let radar = if c.track_id < 1 {
                color(255, 0, 0, 255)
            } else {
                color(255, 175, 3, 255)
            };
            if c.lead_two_status > 0 {
                let width = f64::from(integer(c.lead_two_right - c.lead_two_left)?);
                drawing::rounded_box(
                    draw,
                    Rect {
                        x: float(c.lead_two_left - 10.),
                        y: float(c.lead_two_y - width * 0.8),
                        width: float(width + 20.),
                        height: float(width * 0.8),
                    },
                    BoxStyle {
                        fill: if c.lead_two_status == 2 {
                            color(255, 0, 0, 50)
                        } else {
                            color(0, 0, 0, 20)
                        },
                        stroke: color(218, 111, 37, 255),
                        thickness: 3.,
                    },
                )?;
            }
            let width = f64::from(c.path_width);
            drawing::rounded_box(
                draw,
                Rect {
                    x: float(f64::from(c.path_x) - width / 2. - 10.),
                    y: float(f64::from(c.path_y) - width * 0.8),
                    width: float(width + 20.),
                    height: float(width * 0.8),
                },
                BoxStyle {
                    fill: color(0, 0, 0, 20),
                    stroke: if c.track_id >= 0 {
                        radar
                    } else {
                        color(0, 0, 255, 255)
                    },
                    thickness: 3.,
                },
            )?;
        }
        Ok(())
    }
}
