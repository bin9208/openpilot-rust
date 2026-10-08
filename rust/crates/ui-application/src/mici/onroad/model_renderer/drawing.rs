use super::ModelRenderer;
use crate::{
    onroad::{
        model_renderer::{
            colors, hsv,
            input::Input,
            math::{byte, clip, index, interp},
        },
        road_markings,
    },
    paint::color,
    state::Status,
    Error,
};
use openpilot_ui_framework::{
    draw::Draw,
    polygon::{self, Fill, Gradient},
};
impl ModelRenderer {
    fn lane_color(&self, probability: f64, adjacent: bool, left: bool) -> Result<u32, Error> {
        let alpha = byte(clip(probability, 0., 0.7) * 255.)?;
        let ui = self.context.ui.borrow();
        let mut tint = if adjacent {
            let base = match ui.status {
                Status::Disengaged => color(200, 200, 200, alpha),
                Status::Override => color(255, 255, 255, alpha),
                Status::Engaged => color(0, 255, 64, alpha),
            };
            let torque = self.filters.torque;
            if torque.abs() > 0.6 && left == (torque > 0.) {
                hsv::blend(
                    base,
                    color(255, 115, 0, alpha),
                    interp(torque.abs(), &[0.6, 0.8], &[0., 1.])?,
                )?
            } else {
                base
            }
        } else {
            color(255, 255, 255, alpha)
        };
        if ui.status == Status::Disengaged && !ui.lat_active {
            tint = color(0, 0, 0, alpha);
        }
        Ok(tint)
    }
    pub(super) fn draw_lanes(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        for i in 0..4 {
            let code = self.marking_codes[i];
            let probability = self.common.probability(i)?;
            if code >= 0 && probability > 0.3 {
                let tint = if code >= 20 {
                    color(218, 202, 37, 220)
                } else {
                    color(255, 255, 255, 220)
                };
                for points in &self.marking_segments[i] {
                    polygon::polygon(draw, points, (self.widget.rect, Fill::Color(tint)))?;
                }
                continue;
            }
            let line = &self.common.lanes[i].projected;
            if line.is_empty() {
                continue;
            }
            let tint = self.lane_color(f64::from(probability), matches!(i, 1 | 2), i < 2)?;
            polygon::polygon(draw, line, (self.widget.rect, Fill::Color(tint)))?;
        }
        for i in 0..2 {
            let line = &self.common.roads[i].projected;
            if line.is_empty() {
                continue;
            }
            let tint = self.lane_color(
                f64::from(1_f32 - self.common.deviation(i)?),
                f64::from(self.common.probability(i + 1)?) < 0.25,
                i == 0,
            )?;
            polygon::polygon(draw, line, (self.widget.rect, Fill::Color(tint)))?;
        }
        Ok(())
    }
    pub(super) fn draw_blindspots(
        &self,
        input: &Input<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        if !input.valid("carState")?
            || !input.alive("carState")?
            || !input.valid("modelV2")?
            || !input.alive("modelV2")?
        {
            return Ok(());
        }
        let active = [
            input.car.get_left_blindspot(),
            input.car.get_right_blindspot(),
        ];
        let points = &self.common.path.raw;
        if !active.iter().any(|v| *v) || points.len() < 2 {
            return Ok(());
        }
        let end = index(points, 40.);
        for (i, active) in active.into_iter().enumerate() {
            if active {
                let barrier = road_markings::project_blindspot_barrier(
                    &self.common.projection,
                    &points[..end + 1],
                    if i == 0 { -1.7 } else { 1.7 },
                );
                for quad in road_markings::blindspot_barrier_quads(&barrier) {
                    polygon::polygon(
                        draw,
                        &quad,
                        (self.widget.rect, Fill::Color(color(255, 215, 0, 150))),
                    )?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn draw_path(
        &mut self,
        input: &Input<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        if self.common.path.projected.is_empty() {
            return Ok(());
        }
        let throttle = input.longitudinal.get_allow_throttle() || !self.common.longitudinal;
        let alpha = 0.05 / (0.25 + 0.05);
        self.filters.throttle =
            (1. - alpha) * self.filters.throttle + alpha * f64::from(u8::from(throttle));
        if self.common.experimental {
            let fill = if self.gradient.colors.len() > 1 {
                Fill::Gradient(&self.gradient)
            } else {
                Fill::Color(color(255, 255, 255, 30))
            };
            polygon::polygon(draw, &self.common.path.projected, (self.widget.rect, fill))?;
        } else {
            let factor = (self.filters.throttle * 100.).round_ties_even() / 100.;
            let colors = colors::COAST
                .into_iter()
                .zip(colors::THROTTLE)
                .map(|(a, b)| colors::blend(a, b, factor))
                .collect::<Result<_, _>>()?;
            let gradient = Gradient::new((0., 1.), (0., 0.), colors, vec![0., 0.5, 1.]);
            polygon::polygon(
                draw,
                &self.common.path.projected,
                (self.widget.rect, Fill::Gradient(&gradient)),
            )?;
        }
        Ok(())
    }
}
