use super::{front::clear_front, Hyundai};
use crate::{base::Base, Error};

impl Hyundai {
    pub(super) fn update_scc(&mut self, base: &mut Base) -> Result<(), Error> {
        let reader = self
            .rcp_scc
            .as_mut()
            .ok_or(Error::Contract("Hyundai SCC reader absent"))?;
        let address = if self.canfd { 416 } else { 0x420 };
        let distance = reader.signal(address, "ACC_ObjDist")?;
        let velocity = reader.signal(address, "ACC_ObjRelSpd")?;
        let new =
            (distance - self.d_rel_last).abs() > 3. || (velocity - self.v_rel_last).abs() > 1.;
        let lead = velocity + base.v_ego;
        let valid = if self.canfd {
            0. < distance && distance < 150. && !new
        } else {
            reader.signal(address, "ACC_ObjStatus")? != 0. && distance < 150. && !new
        };
        let point = base
            .pts
            .get_mut(&0)
            .ok_or(Error::Contract("Hyundai SCC point absent"))?;
        point.measured = valid;
        if !valid {
            clear_front(point, base.v_ego);
        } else {
            point.d_rel = distance as f32;
            point.y_rel = if self.canfd {
                0.
            } else {
                -reader.signal(address, "ACC_ObjLatPos")? as f32
            };
            point.v_rel = velocity as f32;
            point.v_lead = lead as f32;
            point.a_rel = f32::NAN;
            point.yv_rel = 0.;
        }
        self.d_rel_last = distance;
        self.v_rel_last = velocity;
        Ok(())
    }
}
