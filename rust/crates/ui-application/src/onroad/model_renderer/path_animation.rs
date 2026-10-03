use super::{
    colors,
    drawing::{self, PathPaint},
    input::Input,
    math::integer,
    path_modes::midpoint,
    ModelRenderer,
};
use crate::Error;
use openpilot_ui_framework::draw::Draw;
impl ModelRenderer {
    pub(super) fn draw_animated(
        &mut self,
        input: &Input<'_>,
        draw: &mut dyn Draw,
        paint: PathPaint,
    ) -> Result<(), Error> {
        let length = self.common.path.projected.len();
        if length < 8 {
            return Ok(());
        }
        let speed = f64::from(input.car.get_v_ego()) * 3.6;
        let acceleration = f64::from(input.car.get_a_ego());
        let sequence = (speed / 100.).max(0.3);
        let forward = acceleration >= -1. || acceleration.is_nan();
        let maximum = i32::try_from((length / 4 + 3).min(16))
            .map_err(|_| Error::Contract("path sequence range"))?;
        let second = self.carrot.sequence_second.get_or_insert(-1);
        if forward {
            self.carrot.sequence += sequence;
            if self.carrot.sequence > f64::from(maximum) {
                self.carrot.sequence = if *second >= 0 { f64::from(*second) } else { 0. };
            }
        } else {
            self.carrot.sequence -= sequence;
            if self.carrot.sequence < 0. {
                self.carrot.sequence = if *second >= 0 {
                    f64::from(*second)
                } else {
                    f64::from(maximum)
                };
            }
        }
        let current = integer(self.carrot.sequence)?;
        *second = if maximum > 15 {
            (current - maximum / 2 + maximum).rem_euclid(maximum)
        } else {
            -5
        };
        let second = *second;
        let points = &self.common.path.projected;
        let mode = self.carrot.mode;
        let rectangular = matches!(mode, 1 | 2 | 5 | 6);
        let mut color_index = 0;
        for i in (0..(length / 2).saturating_sub(4)).step_by(2) {
            let step = i32::try_from(i / 2).map_err(|_| Error::Contract("path step range"))?;
            let draw_segment = current == step
                || current == step - 2
                || second == step
                || second == step - 2
                || length / 2 < 8
                || if rectangular {
                    matches!(mode, 5 | 6)
                } else {
                    matches!(mode, 7 | 8)
                };
            if draw_segment {
                let fill = if matches!(mode, 2 | 4 | 6 | 8) {
                    colors::PATH[color_index]
                } else {
                    paint.fill
                };
                let style = PathPaint { fill, ..paint };
                if rectangular {
                    let quad = [
                        points[i],
                        points[i + 2],
                        points[length - i - 3],
                        points[length - i - 1],
                    ];
                    drawing::path_polygon(draw, &quad, style)?;
                } else {
                    let quad = [
                        points[i],
                        points[i + 2],
                        midpoint(points[i + 4], points[length - i - 5]),
                        points[length - i - 3],
                        points[length - i - 1],
                        midpoint(points[i + 2], points[length - i - 3]),
                    ];
                    drawing::two_quads(draw, &quad, style)?;
                }
            }
            if i > 1 {
                color_index += 1;
                if color_index > 6 {
                    color_index = 0;
                }
            }
        }
        Ok(())
    }
}
