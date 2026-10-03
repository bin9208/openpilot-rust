use crate::{params::Read, Error};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealtimeParams {
    pub record_audio: bool,
    pub is_metric: bool,
    pub always_on_dm: bool,
}
impl RealtimeParams {
    pub fn read(params: &impl Read) -> Result<Self, Error> {
        Ok(Self {
            record_audio: params.boolean("RecordAudio")?,
            is_metric: params.boolean("IsMetric")?,
            always_on_dm: params.boolean("AlwaysOnDM")?,
        })
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct CarConfig {
    pub alpha_longitudinal_available: bool,
    pub max_lateral_accel: f64,
    pub openpilot_longitudinal_control: bool,
}
impl CarConfig {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let reader =
            capnp::serialize::read_message(&mut std::io::Cursor::new(bytes), Default::default())?;
        let cp = reader.get_root::<openpilot_cereal::car_capnp::car_params::Reader<'_>>()?;
        Ok(Self {
            alpha_longitudinal_available: cp.get_alpha_longitudinal_available(),
            max_lateral_accel: f64::from(cp.get_max_lateral_accel()),
            openpilot_longitudinal_control: cp.get_openpilot_longitudinal_control(),
        })
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ModelStatus {
    pub compiled: bool,
    pub compile_pending: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct SlowParams {
    pub car: Option<CarConfig>,
    pub has_longitudinal_control: bool,
    pub show_debug_ui: i32,
    pub show_date_time: i32,
    pub show_radar_info: i32,
    pub show_brightness_ratio: f64,
    pub show_model_view: i32,
    pub share_data: bool,
    pub show_camera_with_cluster: bool,
    pub usbgpu_present: bool,
    pub usbgpu_compiled: bool,
    pub usbgpu_compile_pending: bool,
    pub usbgpu_loading: bool,
    pub usbgpu_active: bool,
    pub usbgpu_startup_failed: bool,
}
impl SlowParams {
    pub fn refresh(&mut self, params: &impl Read, models: ModelStatus) -> Result<(), Error> {
        if let Some(bytes) = params.bytes("CarParamsPersistent")? {
            let car = CarConfig::decode(&bytes)?;
            self.has_longitudinal_control = if car.alpha_longitudinal_available {
                params.boolean("AlphaLongitudinalEnabled")?
            } else {
                car.openpilot_longitudinal_control
            };
            self.car = Some(car);
        }
        self.show_debug_ui = params.integer("ShowDebugUI")?;
        self.show_date_time = params.integer("ShowDateTime")?;
        self.show_radar_info = params.integer("ShowRadarInfo")?;
        self.show_brightness_ratio = f64::from(params.integer("ShowCustomBrightness")?) / 100.0;
        self.show_model_view = params.integer("ShowModelView")?;
        self.share_data = params.boolean("ShareData")?;
        self.show_camera_with_cluster = params.integer("ShowCameraWithCluster")? == 1;
        self.usbgpu_present = params.boolean("UsbGpuPresent")?;
        self.usbgpu_compiled = models.compiled;
        self.usbgpu_compile_pending = models.compile_pending;
        self.usbgpu_loading = params.boolean("UsbGpuLoading")?;
        self.usbgpu_active = params.boolean("UsbGpuActive")?;
        self.usbgpu_startup_failed = params.boolean("UsbGpuStartupFailed")?;
        Ok(())
    }
}
