use super::{cached, Input};
use crate::{live::Error, models::LiveDebugInfo};

impl Input {
    pub(in crate::live) fn live_debug_info(&self) -> Result<Option<LiveDebugInfo>, Error> {
        let mut info = self.debug.cached.clone().unwrap_or_default();
        let delay = self.subscriber.state.topic("liveDelay")?;
        if delay.alive {
            let data = self.data("liveDelay")?;
            info.live_delay_calibration_percent = data
                .field("calPerc")
                .float()
                .or(info.live_delay_calibration_percent);
            info.live_delay_lateral_s = data
                .field("lateralDelay")
                .float()
                .or(info.live_delay_lateral_s);
        }
        let torque = self.subscriber.state.topic("liveTorqueParameters")?;
        if torque.alive {
            let data = self.data("liveTorqueParameters")?;
            info.live_torque_calibration_percent = data
                .field("calPerc")
                .float()
                .or(info.live_torque_calibration_percent);
            info.live_torque_valid = data.field("liveValid").boolean().or(info.live_torque_valid);
            info.live_torque_lat_accel_factor = data
                .field("latAccelFactorFiltered")
                .float()
                .or(info.live_torque_lat_accel_factor)
                .or_else(|| data.field("latAccelFactor").float());
            info.live_torque_friction = data
                .field("frictionCoefficientFiltered")
                .float()
                .or(info.live_torque_friction)
                .or_else(|| data.field("frictionCoefficient").float());
        }
        if self.subscriber.state.topic("liveParameters")?.alive {
            info.live_steer_ratio = self
                .data("liveParameters")?
                .field("steerRatio")
                .float()
                .or(info.live_steer_ratio);
        }
        info.custom_steer_ratio = self.debug.custom_ratio;
        info.steer_actuator_delay_s = self
            .debug
            .actuator_delay
            .filter(|delay| *delay > 0.0)
            .or(info.live_delay_lateral_s)
            .or(self.debug.actuator_delay);
        Ok(cached::present(&info).then_some(info))
    }
}
