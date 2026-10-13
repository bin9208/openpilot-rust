use super::{Error, Input};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::event;

fn rounded(value: f64) -> Option<i32> {
    value
        .is_finite()
        .then(|| value.clamp(0.0, 100.0).round_ties_even().to_i32())
        .flatten()
}

impl Input {
    /// # Errors
    /// Returns a malformed subscribed Cereal event or subscription-state failure.
    pub fn brightness(&self) -> Result<Option<i32>, Error> {
        let wide = self.subscriber.state.topic("wideRoadCameraState")?;
        if wide.alive && wide.valid {
            if let event::WideRoadCameraState(data) = wide.event()?.which()? {
                let exposure = f64::from(data?.get_exposure_val_percent());
                if exposure.is_finite() {
                    let lightness = (100.0 - exposure).clamp(0.0, 100.0);
                    let normalized = if lightness <= 8.0 {
                        lightness / 903.3
                    } else {
                        ((lightness + 16.0) / 116.0).powi(3)
                    };
                    return Ok(rounded(30.0 + normalized.clamp(0.0, 1.0) * 70.0));
                }
            }
        }
        let device = self.subscriber.state.topic("deviceState")?;
        if !device.alive {
            return Ok(None);
        }
        match device.event()?.which()? {
            event::DeviceState(data) => {
                let data = data?;
                Ok(if data.get_started() {
                    rounded(f64::from(data.get_screen_brightness_percent()))
                } else {
                    Some(0)
                })
            }
            _ => Ok(None),
        }
    }
}
