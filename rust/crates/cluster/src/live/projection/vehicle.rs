use super::{Data, Projection};
use num_traits::ToPrimitive;

impl Projection {
    pub(super) fn odometry(&mut self, camera: Data<'_>, valid: bool) {
        self.state.camera_odometry_valid = Some(valid);
        if let Some(value) = camera
            .field("trans")
            .index(0)
            .float()
            .filter(|value| value.abs() < 90.0)
        {
            self.state.vision_speed_mps = Some(value);
        }
        if let Some(value) = camera
            .field("rot")
            .index(2)
            .float()
            .filter(|value| value.abs() < 2.0)
        {
            self.state.vision_yaw_rate_rps = Some(value);
        }
        if let Some(value) = camera.field("transStd").index(0).float() {
            self.state.vision_speed_std_mps = Some(value.clamp(0.0, 20.0));
        }
        if let Some(value) = camera.field("rotStd").index(2).float() {
            self.state.vision_yaw_rate_std_rps = Some(value.clamp(0.0, 2.0));
        }
        if self.state.wide_camera_from_device_euler.is_none() {
            self.state.wide_camera_from_device_euler = camera.field("wideFromDeviceEuler").triple();
        }
        if self.state.road_transform_trans.is_none() {
            self.state.road_transform_trans = camera.field("roadTransformTrans").triple();
        }
        self.state.road_transform_std = camera.field("roadTransformTransStd").triple();
    }
    pub(super) fn calibration(&mut self, calibration: Data<'_>, valid: bool) {
        if !valid {
            return;
        }
        if let Some(value) = calibration.field("rpyCalib").triple() {
            self.state.camera_calibration_euler = Some(value);
        }
        if let Some(value) = calibration.field("wideFromDeviceEuler").triple() {
            self.state.wide_camera_from_device_euler = Some(value);
        }
        if let Some(height) = calibration.field("height").numbers(1, 0.5, 3.0).first() {
            self.state.road_transform_trans = Some([0.0, 0.0, *height]);
        }
    }
    pub(super) fn device(&mut self, device: Data<'_>) {
        if let Some(name) = device.field("deviceType").text() {
            self.state.camera_device_type = Some(name.trim().to_lowercase());
        }
        let usages = device.field("cpuUsagePercent").numbers(64, 0.0, 100.0);
        if !usages.is_empty() {
            self.state.cpu_usage_percent =
                Some(usages.iter().sum::<f64>() / usages.len().to_f64().unwrap_or(1.0));
        }
        let temperatures = device.field("cpuTempC").numbers(64, -50.0, 200.0);
        if let Some(value) = temperatures.into_iter().reduce(f64::max) {
            self.state.cpu_temp_c = Some(value);
        }
        if let Some(value) = device
            .field("memoryUsagePercent")
            .float()
            .filter(|value| (0.0..=100.0).contains(value))
        {
            self.state.memory_used_percent = Some(value);
        }
        if let Some(value) = device
            .field("freeSpacePercent")
            .float()
            .filter(|value| (0.0..=100.0).contains(value))
        {
            self.state.disk_used_percent = Some(100.0 - value);
        }
    }
    pub(super) fn car_params(&mut self, params: Data<'_>) {
        self.radar.car_params(params);
        if let Some(value) = params
            .field("wheelbase")
            .float()
            .filter(|value| (1.5..=5.0).contains(value))
        {
            self.wheelbase = value;
        }
        if let Some(value) = params
            .field("steerRatio")
            .float()
            .filter(|value| (1.0..=40.0).contains(value))
        {
            self.steer_ratio = value;
        }
    }
}
