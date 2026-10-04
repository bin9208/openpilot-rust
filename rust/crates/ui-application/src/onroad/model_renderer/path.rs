use super::{
    colors,
    common::read_points,
    drawing::{self, PathPaint},
    input::Input,
    math::{clip, index, integer},
    path_sampling,
    points::SamplePoint,
    ModelRenderer,
};
use crate::{
    onroad::path_geometry::{self, PathRibbon},
    Error,
};
use openpilot_ui_framework::{draw::Draw, polygon};
use std::borrow::Cow;
impl ModelRenderer {
    fn make_path(&mut self, input: &Input<'_>) -> Result<bool, Error> {
        if !input.valid("modelV2")? || !input.valid("carState")? {
            return Ok(false);
        }
        self.carrot.active_lane = input.controls.get_active_lane_line();
        let line = if self.carrot.active_lane && input.valid("lateralPlan")? {
            Cow::Owned(read_points(input.lateral.get_position()?)?)
        } else {
            Cow::Borrowed(self.common.path.raw.as_slice())
        };
        if line.is_empty() {
            return Ok(false);
        }
        let maximum = clip(f64::from(line[line.len() - 1].0[0]), 10., 100.) - 2.;
        let last = index(&line, maximum);
        self.carrot.long_active = input.selfdrive.get_enabled();
        (self.carrot.mode, self.carrot.color) = if self.carrot.active_lane {
            (self.settings.lane_mode, self.settings.lane_color)
        } else {
            (self.settings.normal_mode, self.settings.normal_color)
        };
        if !self.carrot.long_active {
            self.carrot.color = self.settings.cruise_off_color;
        }
        let samples = if self.carrot.mode == 0 {
            line[..last + 1]
                .iter()
                .filter(|p| p.0[0] >= 0.)
                .copied()
                .map(SamplePoint::from)
                .collect()
        } else {
            let distances = if self.carrot.mode < 9 || (13..=15).contains(&self.carrot.mode) {
                path_sampling::geometric_distances(maximum)
            } else {
                path_sampling::animated_distances(
                    &mut self.carrot,
                    f64::from(input.car.get_v_ego_cluster()),
                    maximum,
                )?
            };
            path_geometry::sample_path(&line, &distances)?
        };
        let invert = self.carrot.mode == 0
            || (self.carrot.mode >= 9 && !(13..=15).contains(&self.carrot.mode));
        self.common.path.projected = path_geometry::project_path(
            &self.common.projection,
            &samples,
            PathRibbon {
                width: self.carrot.width,
                height: [1.22, 1.22],
                allow_invert: invert,
            },
        )?;
        Ok(!self.common.path.projected.is_empty())
    }
    pub(super) fn draw_path(
        &mut self,
        input: &Input<'_>,
        now: f64,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.settings.refresh(self.context.params.as_ref(), now)?;
        if !self.make_path(input)? {
            return Ok(());
        }
        self.update_path_end(input)?;
        self.carrot.lane_speed = integer(f64::from(input.car.get_use_lane_line_speed()))?;
        let brake = input.car.get_brake_lights();
        let accel = input
            .longitudinal
            .get_accels()?
            .iter()
            .next()
            .map_or(0., f64::from);
        let mut color = self.carrot.color;
        if color >= 20 {
            color = if self.carrot.long_active {
                if input.valid("radarState")? && input.radar.get_lead_one()?.get_status() {
                    if accel.abs() < 0.5 {
                        12
                    } else if accel >= 0.5 {
                        11
                    } else {
                        10
                    }
                } else {
                    13
                }
            } else {
                19
            };
        }
        let paint = PathPaint {
            fill: colors::path(color)?,
            brake,
            color_index: color,
        };
        match self.carrot.mode {
            0 => {
                polygon::solid(draw, &self.common.path.projected, paint.fill)?;
                drawing::path_outline(draw, &self.common.path.projected, &paint)?;
            }
            13..=15 => self.draw_special(draw, paint)?,
            mode if mode >= 9 => self.draw_complex(draw, paint)?,
            _ => self.draw_animated(input, draw, paint)?,
        }
        self.draw_path_end(draw)?;
        self.draw_tires(input, now, draw)
    }
}
