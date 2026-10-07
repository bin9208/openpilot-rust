use super::{
    drawing::{self, Label},
    input::Input,
    math::{byte, index},
    points::{SamplePoint, ScreenPoint},
    projection::Ribbon,
    ModelRenderer,
};
use crate::{paint::color, Error};
use openpilot_ui_framework::{
    draw::Draw,
    polygon::{self, Fill, Gradient},
};
fn danger_color(danger: f64) -> Result<u32, Error> {
    let d = danger.clamp(0., 1.5);
    let (r, g) = if d <= 0.1 {
        (0., 200.)
    } else if d <= 0.6 {
        ((d - 0.1) / 0.5 * 255., 200.)
    } else if d <= 1. {
        (255., 200. - (d - 0.6) / 0.4 * 200.)
    } else {
        (255., 0.)
    };
    Ok(color(byte(r)?, byte(g)?, 0, 255))
}
impl ModelRenderer {
    pub(super) fn draw_tires(
        &self,
        input: &Input<'_>,
        now: f64,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        if self.settings.tire_trajectory <= 0 || !input.valid("modelV2")? {
            return Ok(());
        }
        let lanes = input.model.get_lane_lines()?;
        let probs = input.model.get_lane_line_probs()?;
        if lanes.len() < 3 || probs.len() < 3 {
            return Ok(());
        }
        let left = lanes.get(1).get_y()?;
        let right = lanes.get(2).get_y()?;
        if left.is_empty() || right.is_empty() {
            return Ok(());
        }
        let mut left = f64::from(left.get(0));
        let mut right = -f64::from(right.get(0));
        if f64::from(probs.get(1)) < 0.3 && f64::from(probs.get(2)) > 0.3 {
            left = 3. - right;
        } else if f64::from(probs.get(2)) < 0.3 && f64::from(probs.get(1)) > 0.3 {
            right = 3. - left;
        }
        let mut total = left + right;
        if total < 0.5 {
            total = 3.;
        }
        let drift = (right - left) / 2.;
        let limit = (total / 2. - 0.9).max(0.1);
        let danger = drift.abs() / limit;
        let label = if drift > 0.02 {
            "L"
        } else if drift < -0.02 {
            "R"
        } else {
            "C"
        };
        let path = &self.common.path.raw;
        if path.len() < 5 {
            return Ok(());
        }
        let mut maximum = 40.;
        if input.valid("radarState")? {
            let lead = input.radar.get_lead_one()?;
            let distance = f64::from(lead.get_d_rel());
            if lead.get_status() && distance > 3. && distance < maximum {
                maximum = distance;
            }
        }
        let end = index(path, maximum);
        if end < 5 {
            return Ok(());
        }
        let green = color(0, 200, 0, 255);
        let warn = drift.abs() >= 0.5;
        let pulse = warn && (now * 1000.).trunc().rem_euclid(800.) < 400.;
        let (left, right) = if warn {
            let side = if pulse {
                color(255, 0, 0, 255)
            } else {
                color(255, 100, 0, 220)
            };
            (
                if drift > 0. { side } else { green },
                if drift < 0. { side } else { green },
            )
        } else if drift > 0.02 {
            (danger_color(danger)?, green)
        } else if drift < -0.02 {
            (green, danger_color(danger)?)
        } else {
            (green, green)
        };
        for (shift, tint) in [(0.9, left), (-0.9, right)] {
            let ribbon = self.common.projection.ribbon(
                path,
                Ribbon {
                    half_width: 0.16,
                    height: 1.22,
                    shift,
                    end,
                    end_distance: Some(maximum),
                    allow_invert: true,
                },
            )?;
            if ribbon.is_empty() {
                continue;
            }
            let [r, g, b, _] = tint.to_le_bytes();
            let gradient = Gradient::new(
                (0., 1.),
                (0., 0.),
                vec![tint, color(r, g, b, 0)],
                vec![0., 1.],
            );
            polygon::polygon(draw, &ribbon, (self.widget.rect, Fill::Gradient(&gradient)))?;
        }
        if danger < 1.5 {
            if let Some(base) = self.common.projection.point(SamplePoint([
                3.,
                f64::from(path[0].0[1]),
                f64::from(path[0].0[2]) + self.common.path_height,
            ])) {
                drawing::text(
                    draw,
                    Label::center(label, ScreenPoint([base.0[0], base.0[1] - 56.]), 72.),
                )?;
                drawing::text(
                    draw,
                    Label::center(
                        &format!("{:.2}", drift.abs()),
                        ScreenPoint([base.0[0], base.0[1] + 18.]),
                        52.,
                    ),
                )?;
            }
        }
        Ok(())
    }
}
