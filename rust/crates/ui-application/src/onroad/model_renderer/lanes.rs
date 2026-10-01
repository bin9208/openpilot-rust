use super::{
    drawing,
    input::Input,
    math::{byte, clip, index},
    projection::Ribbon,
    ModelRenderer,
};
use crate::{onroad::road_markings, paint::color, Error};
use openpilot_ui_framework::{draw::Draw, polygon};
impl ModelRenderer {
    pub(super) fn draw_lanes(&self, input: &Input<'_>, draw: &mut dyn Draw) -> Result<(), Error> {
        if self.settings.lane_info < 1 || !input.valid("modelV2")? || !input.valid("carState")? {
            return Ok(());
        }
        let zero = &self.common.lanes[0].raw;
        if zero.is_empty()
            || (self.settings.lane_info == 1 && !self.common.probabilities.iter().any(|p| *p > 0.3))
        {
            return Ok(());
        }
        let maximum = clip(f64::from(zero[zero.len() - 1].0[0]), 10., 100.);
        let end = index(zero, maximum);
        let left = input.car.get_left_lane_line();
        let right = input.car.get_right_lane_line();
        let double_left = left.rem_euclid(10) == 4;
        for i in 0..4 {
            if self.common.probability(i)?.partial_cmp(&0.3) != Some(std::cmp::Ordering::Greater) {
                continue;
            }
            let line = &self.common.lanes[i].raw;
            let code = match i {
                1 => Some(left),
                2 => Some(right),
                _ => None,
            };
            let width = if i == 1 && left >= 20 { 0.05 } else { 0.025 };
            let shape = Ribbon {
                half_width: width,
                height: 0.,
                shift: 0.,
                end,
                end_distance: Some(maximum),
                allow_invert: true,
            };
            let segments = if code.is_some_and(|c| c >= 0 && c.rem_euclid(10) == 0) {
                road_markings::project_lane_segments(
                    &self.common.projection,
                    &road_markings::lane_dash_segments(line, maximum)?,
                    width,
                )
            } else {
                let vertices = self.common.projection.ribbon(line, shape)?;
                if vertices.is_empty() {
                    Vec::new()
                } else {
                    vec![vertices]
                }
            };
            if segments.is_empty() {
                continue;
            }
            let tint = if (i == 1 && left >= 20) || (i == 2 && right >= 20) {
                color(218, 202, 37, 220)
            } else {
                color(255, 255, 255, 220)
            };
            let stroke = i == 1 && left >= 20;
            for segment in segments {
                polygon::solid(draw, &segment, tint)?;
                if stroke {
                    drawing::outline(draw, &segment, (tint, 1.))?;
                }
            }
            if i == 1 && double_left {
                let vertices = self.common.projection.ribbon(
                    line,
                    Ribbon {
                        shift: -0.3,
                        ..shape
                    },
                )?;
                if !vertices.is_empty() {
                    polygon::solid(draw, &vertices, tint)?;
                    if stroke {
                        drawing::outline(draw, &vertices, (tint, 1.))?;
                    }
                }
            }
        }
        if self.settings.lane_info > 1 {
            let end = index(zero, 100.);
            for i in 0..2 {
                let vertices = self.common.projection.ribbon(
                    &self.common.roads[i].raw,
                    Ribbon {
                        half_width: 0.025,
                        height: 0.,
                        shift: 0.,
                        end,
                        end_distance: Some(100.),
                        allow_invert: true,
                    },
                )?;
                if vertices.is_empty() {
                    continue;
                }
                let fraction = clip(f64::from(self.common.deviation(i)? / 2.), 0., 1.);
                polygon::solid(
                    draw,
                    &vertices,
                    color(
                        byte((1. - fraction) * 255.)?,
                        0,
                        byte(fraction * 255.)?,
                        255,
                    ),
                )?;
            }
        }
        Ok(())
    }
}
