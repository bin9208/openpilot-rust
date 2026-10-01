use super::{
    drawing::{self, PathPaint},
    input::Input,
    math::index,
    ModelRenderer,
};
use crate::{onroad::road_markings, paint::color, Error};
use openpilot_cereal::log_capnp::{LaneChangeDirection, LaneChangeState};
use openpilot_ui_framework::draw::Draw;
impl ModelRenderer {
    pub(super) fn draw_blindspot(
        &mut self,
        input: &Input<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        if !input.valid("modelV2")? || !input.valid("carState")? || !input.valid("radarState")? {
            return Ok(());
        }
        let meta = input.model.get_meta()?;
        let preparing = meta.get_lane_change_state()? == LaneChangeState::PreLaneChange;
        let direction = meta.get_lane_change_direction()?;
        let warning = [
            input.car.get_left_blindspot(),
            input.car.get_right_blindspot(),
        ];
        let leads = [input.radar.get_lead_left()?, input.radar.get_lead_right()?];
        let assist_distance = f64::from(input.car.get_v_ego()) * 3.;
        let assist = std::array::from_fn::<_, 2, _>(|i| {
            !warning[i]
                && leads[i].get_status()
                && f64::from(leads[i].get_d_rel()) < assist_distance
                && preparing
                && direction
                    == if i == 0 {
                        LaneChangeDirection::Left
                    } else {
                        LaneChangeDirection::Right
                    }
        });
        if !warning.iter().chain(assist.iter()).any(|v| *v) {
            return Ok(());
        }
        let points = &self.common.path.raw;
        if points.is_empty() {
            self.carrot.barriers.iter_mut().for_each(Vec::clear);
            return Ok(());
        }
        let end = index(points, 40.);
        for i in 0..2 {
            if warning[i] || assist[i] {
                self.carrot.barriers[i] = road_markings::project_blindspot_barrier(
                    &self.common.projection,
                    &points[..end + 1],
                    if i == 0 { -1.7 } else { 1.7 },
                );
                let tint = if warning[i] {
                    color(255, 215, 0, 150)
                } else {
                    color(0, 204, 0, 150)
                };
                for quad in road_markings::blindspot_barrier_quads(&self.carrot.barriers[i]) {
                    drawing::path_polygon(
                        draw,
                        &quad,
                        PathPaint {
                            fill: tint,
                            brake: false,
                            color_index: 10,
                        },
                    )?;
                }
            }
        }
        Ok(())
    }
}
