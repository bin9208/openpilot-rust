use super::{
    input::Input,
    math::{clip, index, integer},
    points::SamplePoint,
    ModelRenderer,
};
use crate::Error;
impl ModelRenderer {
    pub(super) fn update_path_end(&mut self, input: &Input<'_>) -> Result<(), Error> {
        let line = &self.common.path.raw;
        if line.is_empty() {
            return Ok(());
        }
        let lead_one = if input.valid("radarState")? {
            Some(input.radar.get_lead_one()?)
        } else {
            None
        };
        let lead_two = if input.valid("radarState")? {
            Some(input.radar.get_lead_two()?)
        } else {
            None
        };
        let c = &mut self.carrot;
        c.speed = f64::from(input.car.get_v_ego());
        c.brake_hold = input.car.get_brake_hold_active();
        c.soft_hold = input.car.get_soft_hold_active();
        c.cruise = input.car.get_carrot_cruise();
        c.long_active = input.selfdrive.get_enabled();
        c.x_state = input.longitudinal.get_x_state();
        c.traffic_state = input.longitudinal.get_traffic_state();
        let mut distance = clip(f64::from(line[line.len() - 1].0[0]), 10., 100.);
        let idx = index(line, distance);
        let mut y = f64::from(line[idx].0[1]);
        let mut z = f64::from(line[idx].0[2]);
        let vision = input.model.get_leads_v3()?;
        c.vision_distance = if !vision.is_empty() && vision.get(0).get_prob() > 0.5 {
            let xs = vision.get(0).get_x()?;
            if xs.is_empty() {
                return Err(Error::Contract("vision lead position missing"));
            }
            f64::from(xs.get(0)) - 1.52
        } else {
            0.
        };
        c.lead_status = false;
        c.track_id = -1;
        c.radar_distance = 0.;
        if let Some(lead) = lead_one.filter(|lead| lead.get_status()) {
            z = f64::from(line[index(line, f64::from(lead.get_d_rel()))].0[2]);
            distance = f64::from(lead.get_d_rel());
            y = -f64::from(lead.get_y_rel());
            c.track_id = lead.get_radar_track_id();
            c.radar_distance = if lead.get_radar() { distance } else { 0. };
            c.lead_status = true;
        }
        let projection = &self.common.projection;
        let left = projection.point(SamplePoint([distance, y - 1.2, z + 1.22]));
        let right = projection.point(SamplePoint([distance, y + 1.2, z + 1.22]));
        if let (Some(left), Some(right)) = (left, right) {
            let width = right.0[0] - left.0[0];
            let x = clip(
                (left.0[0] + right.0[0]) / 2.,
                350.,
                f64::from(self.widget.rect.width) - 350.,
            );
            let y = clip(
                (left.0[1] + right.0[1]) / 2.,
                200.,
                f64::from(self.widget.rect.height) - 80.,
            );
            c.filtered_x = c.filtered_x * 0.85 + x * (1. - 0.85);
            c.filtered_y = c.filtered_y * 0.85 + y * (1. - 0.85);
            let width = clip(width, 120., 800.);
            c.filtered_width = c.filtered_width * 0.85 + width * (1. - 0.85);
            c.path_x = integer(c.filtered_x)?;
            c.path_y = integer(c.filtered_y)?;
            c.path_width = integer(c.filtered_width)?;
        }
        c.t_follow = f64::from(input.longitudinal.get_t_follow());
        c.follow_distance = f64::from(input.longitudinal.get_desired_distance());
        let point = line[index(line, c.follow_distance)].0;
        c.follow_left = projection.point(SamplePoint([
            c.follow_distance,
            f64::from(point[1]) - 1.,
            f64::from(point[2]) + 1.22,
        ]));
        c.follow_right = projection.point(SamplePoint([
            c.follow_distance,
            f64::from(point[1]) + 1.,
            f64::from(point[2]) + 1.22,
        ]));
        if let (Some(one), Some(two)) = (lead_one, lead_two) {
            if two.get_radar() && f64::from(two.get_d_rel()) > f64::from(one.get_d_rel()) + 3. {
                let z = f64::from(line[index(line, f64::from(two.get_d_rel()))].0[2]);
                let y = -f64::from(two.get_y_rel());
                let left =
                    projection.point(SamplePoint([f64::from(two.get_d_rel()), y - 1.2, z + 1.22]));
                let right =
                    projection.point(SamplePoint([f64::from(two.get_d_rel()), y + 1.2, z + 1.22]));
                if let (Some(left), Some(right)) = (left, right) {
                    if c.lead_two_status > 0 {
                        c.lead_two_left = c.lead_two_left * 0.8 + left.0[0] * 0.2;
                        c.lead_two_right = c.lead_two_right * 0.8 + right.0[0] * 0.2;
                        c.lead_two_y = c.lead_two_y * 0.8 + left.0[1] * 0.2;
                    } else {
                        c.lead_two_left = left.0[0];
                        c.lead_two_right = right.0[0];
                        c.lead_two_y = left.0[1];
                    }
                    c.lead_two_status =
                        if u16::from(input.longitudinal.get_longitudinal_plan_source()?) == 1 {
                            2
                        } else {
                            1
                        };
                }
            } else {
                c.lead_two_status = 0;
            }
        } else {
            c.lead_two_status = 0;
        }
        Ok(())
    }
}
