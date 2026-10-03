use crate::{inputs::CarState, parameters::Parameters, Error};
#[derive(Default)]
pub struct Suspend {
    pub active: bool,
    pub enter: f64,
    pub hold: f64,
}
impl Suspend {
    pub fn update(
        &mut self,
        car: &CarState,
        active: bool,
        params: &mut impl Parameters,
    ) -> Result<bool, Error> {
        let threshold = f64::from(params.integer("LatSuspendAngleDeg")?);
        if !self.active {
            if car.steering_pressed && car.steer_angle.abs() > threshold {
                self.enter += 0.01;
                if self.enter >= 1. {
                    self.active = true;
                    self.hold = 0.;
                }
            } else {
                self.enter = 0.;
            }
        }
        if self.active {
            self.hold += 0.01;
            if self.hold >= 0.5 && car.steer_angle.abs() < 15. && !car.steering_pressed {
                self.active = false;
                self.enter = 0.;
            }
        }
        Ok(active && !self.active)
    }
}
