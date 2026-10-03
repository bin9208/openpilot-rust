use super::ModelRenderer;
use crate::{
    onroad::{
        model_renderer::{
            input::Input,
            math::{clip, float, index},
            points::ModelPoint,
            projection::Ribbon,
        },
        road_markings,
    },
    Error,
};
impl ModelRenderer {
    pub(super) fn update_model(&mut self, input: &Input<'_>) -> Result<(), Error> {
        let mut maximum = clip(
            f64::from(self.common.path.raw[self.common.path.raw.len() - 1].0[0]),
            10.,
            100.,
        );
        let end = index(&self.common.lanes[0].raw, maximum);
        self.marking_codes = if input.valid("carState")? && input.alive("carState")? {
            [
                -1,
                input.car.get_left_lane_line(),
                input.car.get_right_lane_line(),
                -1,
            ]
        } else {
            [-1; 4]
        };
        for i in 0..4 {
            let factor = if i == 0 { 0.12_f32 } else { 0.16 };
            let probability = self.common.probability(i)?;
            let shape = Ribbon {
                half_width: f64::from(factor * probability),
                height: 0.,
                shift: 0.,
                end,
                end_distance: None,
                allow_invert: true,
            };
            self.common.lanes[i].projected = self
                .common
                .projection
                .ribbon(&self.common.lanes[i].raw, shape)?;
            self.marking_segments[i].clear();
            let code = self.marking_codes[i];
            if code >= 0 && probability > 0.3 {
                let line = &self.common.lanes[i].raw;
                let dashed = code.rem_euclid(10) == 0;
                if dashed {
                    for segment in road_markings::lane_dash_segments(line, maximum)? {
                        let points = self.common.projection.ribbon(
                            &segment,
                            Ribbon {
                                half_width: 0.05,
                                end: segment.len().saturating_sub(1),
                                ..shape
                            },
                        )?;
                        if !points.is_empty() {
                            self.marking_segments[i].push(points);
                        }
                    }
                } else {
                    let points = self.common.projection.ribbon(
                        line,
                        Ribbon {
                            half_width: 0.05,
                            ..shape
                        },
                    )?;
                    if !points.is_empty() {
                        self.marking_segments[i].push(points);
                    }
                }
                if i == 1 && code.rem_euclid(10) == 4 {
                    let shifted: Vec<_> = line
                        .iter()
                        .map(|p| ModelPoint([p.0[0], p.0[1] - 0.3_f32, p.0[2]]))
                        .collect();
                    let points = self.common.projection.ribbon(
                        &shifted,
                        Ribbon {
                            half_width: 0.05,
                            ..shape
                        },
                    )?;
                    if !points.is_empty() {
                        self.marking_segments[i].push(points);
                    }
                }
            }
        }
        for edge in &mut self.common.roads {
            edge.projected = self.common.projection.ribbon(
                &edge.raw,
                Ribbon {
                    half_width: 0.16,
                    height: 0.,
                    shift: 0.,
                    end,
                    end_distance: None,
                    allow_invert: true,
                },
            )?;
        }
        if input.valid("radarState")? {
            let lead = input.radar.get_lead_one()?;
            if lead.get_status() {
                let distance = f64::from(lead.get_d_rel()) * 2.;
                maximum = f64::from(float(clip(
                    distance - (distance * 0.35).min(10.),
                    0.,
                    maximum,
                )));
            }
        }
        let acceleration = self
            .common
            .acceleration
            .get(self.common.acceleration.len() / 4)
            .copied()
            .unwrap_or(0.);
        let alpha = 0.05 / (0.1 + 0.05);
        let slow = 0.05 / (1. + 0.05);
        self.filters.acceleration =
            float(1. - alpha) * self.filters.acceleration + float(alpha) * acceleration;
        self.filters.acceleration_slow =
            float(1. - slow) * self.filters.acceleration_slow + float(slow) * acceleration;
        let end = index(&self.common.path.raw, maximum);
        self.common.path.projected = self.common.projection.ribbon(
            &self.common.path.raw,
            Ribbon {
                half_width: 0.9,
                height: self.common.path_height,
                shift: 0.,
                end,
                end_distance: None,
                allow_invert: false,
            },
        )?;
        self.update_gradient()
    }
}
