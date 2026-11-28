//! Required current calibration, camera identity and angular-motion context.
use super::transport::Service;
#[derive(Debug, serde::Deserialize)]
pub struct Context {
    pub calibration: Service,
    pub camera: Service,
    pub device: Service,
    pub pose: Service,
    pub calibrated: bool,
    pub device_type: String,
    pub sensor: String,
    pub rpy: Vec<f64>,
    pub pose_timestamp_ns: u64,
    pub inputs_ok: bool,
    pub posenet_ok: bool,
    pub sensors_ok: bool,
    pub angular_valid: bool,
    pub orientation_valid: bool,
    pub angular: Vec<f64>,
    pub angular_std: Vec<f64>,
}
#[derive(Debug, serde::Serialize)]
pub struct GeometryContext {
    pub focal: f64,
    pub rpy: [f64; 3],
    pub turning: bool,
}
/// Focal length (px) of the 1344x760 signal view: the OS04C10 road camera
/// natively; the 1928x1208 AR0231/OX03C10 camera (focal 2648) after
/// signal-observer scales it by 1344/1928 and keeps the middle rows.
pub fn focal(sensor: &str) -> Option<f64> {
    match sensor {
        "os04c10" => Some(1141.5),
        "ar0231" | "ox03c10" => Some(2648. * 1344. / 1928.),
        _ => None,
    }
}
impl Context {
    pub fn geometry(&self, now: f64) -> Option<GeometryContext> {
        if !self.calibration.fresh(now, 2.)
            || !self.camera.fresh(now, 0.25)
            || !self.device.fresh(now, 2.)
            || !self.calibrated
            || !matches!(self.device_type.as_str(), "tici" | "tizi" | "mici")
            || !matches!(self.sensor.as_str(), "os04c10" | "ar0231" | "ox03c10")
            || self.rpy.len() != 3
            || !self.rpy.iter().all(|v| v.is_finite())
            || !self.pose.fresh(now, 0.2)
            || !(0. ..=0.2).contains(&(now - self.pose_timestamp_ns as f64 / 1e9))
            || !self.inputs_ok
            || !self.posenet_ok
            || !self.sensors_ok
            || !self.angular_valid
            || !self.orientation_valid
            || self.angular.len() != 3
            || self.angular_std.len() != 3
            || !self
                .angular
                .iter()
                .chain(&self.angular_std)
                .all(|v| v.is_finite())
            || self.angular_std.iter().any(|v| *v < 0.)
        {
            return None;
        }
        let rpy: [f64; 3] = self.rpy.as_slice().try_into().ok()?;
        let rotation = openpilot_calibrationd::orientation::rotation(rpy);
        let yaw = (rotation[0][2] * self.angular[0]
            + rotation[1][2] * self.angular[1]
            + rotation[2][2] * self.angular[2])
            .to_degrees();
        let std = (self.angular_std[0] * self.angular_std[0]
            + self.angular_std[1] * self.angular_std[1]
            + self.angular_std[2] * self.angular_std[2])
            .sqrt()
            .to_degrees();
        Some(GeometryContext {
            focal: focal(&self.sensor)?,
            rpy,
            turning: yaw.abs() > 1.5 || std > 2.,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_full_road_camera_uses_its_scaled_focal() {
        assert_eq!(super::focal("os04c10"), Some(1141.5));
        let scaled = super::focal("ox03c10").unwrap();
        assert!((scaled - 1845.91).abs() < 0.01);
        assert_eq!(super::focal("ar0231"), Some(scaled));
        assert_eq!(super::focal("unsupported"), None);
    }
}
