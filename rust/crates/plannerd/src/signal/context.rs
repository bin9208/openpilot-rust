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
impl Context {
    pub fn geometry(&self, now: f64) -> Option<GeometryContext> {
        if !self.calibration.fresh(now, 2.)
            || !self.camera.fresh(now, 0.25)
            || !self.device.fresh(now, 2.)
            || !self.calibrated
            || !matches!(self.device_type.as_str(), "tici" | "tizi" | "mici")
            || self.sensor != "os04c10"
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
            focal: 1141.5,
            rpy,
            turning: yaw.abs() > 1.5 || std > 2.,
        })
    }
}
