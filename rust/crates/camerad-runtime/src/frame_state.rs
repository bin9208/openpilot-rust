use openpilot_camerad::{
    exposure::{CameraId, ExposureError, ExposureState, FrameMeasurement, ManualExposure},
    geometry::{luminance, Geometry, Region, Sampling, SamplingError},
    requests::FrameMetadata,
    sensor::{ExposureRegisters, SensorKind},
};
use openpilot_cereal::log_capnp::{event, frame_data::ImageSensor};

#[derive(Debug, thiserror::Error)]
pub enum FrameStateError {
    #[error(transparent)]
    Exposure(#[from] ExposureError),
    #[error(transparent)]
    Sampling(#[from] SamplingError),
    #[error("raw camera logging requested without a raw image")]
    MissingRaw,
}

pub struct FrameState {
    sensor: SensorKind,
    camera: CameraId,
    exposure: ExposureState,
    region: Region,
    sampling: Sampling,
}

impl FrameState {
    pub fn new(
        sensor: SensorKind,
        camera: CameraId,
        geometry: Geometry,
    ) -> Result<Self, FrameStateError> {
        let width = usize::try_from(geometry.width).map_err(|_| SamplingError)?;
        Ok(Self {
            sensor,
            camera,
            exposure: ExposureState::new(sensor, camera),
            region: geometry.exposure_region(sensor, camera),
            sampling: Sampling {
                width,
                x_skip: 2,
                y_skip: if matches!(camera, CameraId::Driver) {
                    4
                } else {
                    2
                },
            },
        })
    }

    pub fn exposure(&self) -> &ExposureState {
        &self.exposure
    }
    pub fn region(&self) -> Region {
        self.region
    }
    pub fn service(&self) -> &'static str {
        match self.camera {
            CameraId::Wide => "wideRoadCameraState",
            CameraId::Road => "roadCameraState",
            CameraId::Driver => "driverCameraState",
        }
    }

    pub fn wants_raw(&self, frame_id: u32, log_raw: bool) -> bool {
        log_raw && matches!(self.camera, CameraId::Road) && frame_id % 100 == 5
    }

    pub fn encode(
        &self,
        frame: FrameMetadata,
        log_time: u64,
        log_raw: bool,
        raw: Option<&[u8]>,
    ) -> Result<Vec<u8>, FrameStateError> {
        let mut message = capnp::message::Builder::new_default();
        let mut root = message.init_root::<event::Builder<'_>>();
        root.set_valid(true);
        root.set_log_mono_time(log_time);
        let mut data = match self.camera {
            CameraId::Wide => root.init_wide_road_camera_state(),
            CameraId::Road => root.init_road_camera_state(),
            CameraId::Driver => root.init_driver_camera_state(),
        };
        let exposure = &self.exposure;
        data.set_frame_id(frame.frame_id);
        data.set_request_id(frame.request_id);
        data.set_timestamp_eof(frame.timestamp_eof);
        data.set_timestamp_sof(frame.timestamp_sof);
        data.set_integ_lines(exposure.exposure_time);
        data.set_gain(exposure.analog_gain_frac * exposure.gain_factor());
        data.set_high_conversion_gain(exposure.dc_gain_enabled);
        data.set_measured_grey_fraction(exposure.measured_grey_fraction);
        data.set_target_grey_fraction(exposure.target_grey_fraction);
        data.set_processing_time(frame.processing_time);
        let config = self.sensor.config();
        let ev = exposure.cur_ev[(frame.frame_id % 3) as usize].clamp(config.min_ev, config.max_ev);
        data.set_exposure_val_percent(
            (ev - config.min_ev) * 100.0 / (config.max_ev - config.min_ev),
        );
        data.set_sensor(match self.sensor {
            SensorKind::Ar0231 => ImageSensor::Ar0231,
            SensorKind::Ox03c10 => ImageSensor::Ox03c10,
            SensorKind::Os04c10 => ImageSensor::Os04c10,
        });
        if self.wants_raw(frame.frame_id, log_raw) {
            data.set_image(raw.ok_or(FrameStateError::MissingRaw)?);
        }
        Ok(capnp::serialize::write_message_to_words(&message))
    }

    pub fn adjust(
        &mut self,
        frame_id: u32,
        pixels: &[u8],
        enabled: bool,
        manual: ManualExposure<'_>,
    ) -> Result<Option<ExposureRegisters>, FrameStateError> {
        self.adjust_with_manual(frame_id, pixels, enabled, || Ok((manual.gain, manual.time)))
    }

    pub fn adjust_with_manual<G: AsRef<str>, T: AsRef<str>>(
        &mut self,
        frame_id: u32,
        pixels: &[u8],
        enabled: bool,
        read_manual: impl FnOnce() -> Result<(G, T), ExposureError>,
    ) -> Result<Option<ExposureRegisters>, FrameStateError> {
        let grey = luminance(pixels, self.region, self.sampling)?;
        Ok(self.exposure.update_with_manual(
            FrameMeasurement {
                frame_id,
                grey,
                enabled,
            },
            read_manual,
        )?)
    }
}
