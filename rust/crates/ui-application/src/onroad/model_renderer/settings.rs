use crate::{params::Read, Error};
use serde::Serialize;
#[derive(Serialize)]
pub struct Settings {
    pub next_refresh: f64,
    pub lane_info: i32,
    pub radar_info: i32,
    pub normal_mode: i32,
    pub normal_color: i32,
    pub lane_mode: i32,
    pub lane_color: i32,
    pub cruise_off_color: i32,
    pub tire_trajectory: i32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            next_refresh: 0.,
            lane_info: 1,
            radar_info: 0,
            normal_mode: 13,
            normal_color: 14,
            lane_mode: 13,
            lane_color: 14,
            cruise_off_color: 14,
            tire_trajectory: 0,
        }
    }
}
impl Settings {
    pub fn refresh(&mut self, params: &impl Read, now: f64) -> Result<(), Error> {
        if now < self.next_refresh {
            return Ok(());
        }
        self.lane_info = params.integer("ShowLaneInfo")?;
        self.radar_info = params.integer("ShowRadarInfo")?;
        self.normal_mode = params.integer("ShowPathMode")?;
        self.normal_color = params.integer("ShowPathColor")?;
        self.lane_mode = params.integer("ShowPathModeLane")?;
        self.lane_color = params.integer("ShowPathColorLane")?;
        self.cruise_off_color = params.integer("ShowPathColorCruiseOff")?;
        self.tire_trajectory = params.integer("CarrotTireTrajectory")?;
        self.next_refresh = now + 1.;
        Ok(())
    }
}
