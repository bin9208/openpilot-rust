//! Live Cereal input ownership and status policy from `cluster_live.py` (MIT).
mod alerts;
mod brightness;
mod debug;
mod hud;
mod json;
pub mod navigation;
mod params;
mod profile;
pub(crate) mod projection;
mod route;
mod state;
use crate::models::{ClusterAlert, ClusterUiState, EnergyGaugeLabel};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::{runtime::SubMaster, state::Options};
pub use projection::Projection;
use std::time::Duration;

pub const SERVICES: [&str; 26] = [
    "carState",
    "carParams",
    "modelV2",
    "radarState",
    "liveTracks",
    "longitudinalPlan",
    "lateralPlan",
    "controlsState",
    "selfdriveState",
    "carControl",
    "carOutput",
    "deviceState",
    "roadCameraState",
    "cameraOdometry",
    "liveCalibration",
    "livePose",
    "drivingModelData",
    "liveDelay",
    "liveParameters",
    "liveTorqueParameters",
    "navInstruction",
    "navInstructionCarrot",
    "navRoute",
    "carrotMan",
    "carrotNavi",
    "wideRoadCameraState",
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Transport(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Cereal(#[from] capnp::Error),
    #[error(transparent)]
    Schema(#[from] capnp::NotInSchema),
    #[error("cluster input clock is not representable")]
    Clock,
    #[error(transparent)]
    Media(#[from] crate::media::Error),
}

pub struct Input {
    pub subscriber: SubMaster,
    pub frames: u64,
    started: f64,
    timeout: Duration,
    alerts: alerts::Lifecycle,
    projection: Projection,
    navigation: state::Navigation,
    pub last_state: Option<ClusterUiState>,
    energy_label: EnergyGaugeLabel,
    max_lateral_accel: f64,
    params: Option<params::Settings>,
    debug: debug::DebugState,
    route: route::Route,
    media: Option<crate::media::MediaInput>,
    profile: profile::Profile,
}

impl Input {
    /// # Errors
    /// Returns runtime subscription or clock failure.
    pub fn new(include_can: bool, timeout_ms: u64) -> Result<Self, Error> {
        Self::open(include_can, timeout_ms, monotonic()?, false)
    }
    /// # Errors
    /// Returns isolated subscription initialization failure.
    pub fn isolated(include_can: bool, timeout_ms: u64, started: f64) -> Result<Self, Error> {
        Self::open(include_can, timeout_ms, started, true)
    }
    fn open(
        include_can: bool,
        timeout_ms: u64,
        started: f64,
        isolated: bool,
    ) -> Result<Self, Error> {
        let mut services = SERVICES.to_vec();
        if include_can {
            services.extend(["can", "sendcan"]);
        }
        let subscriber = if isolated {
            SubMaster::isolated(&services, Options::default())
        } else {
            SubMaster::for_runtime(&services, Options::default())
        }?;
        let mut input = Self {
            subscriber,
            frames: 0,
            started,
            timeout: Duration::from_millis(timeout_ms),
            alerts: alerts::Lifecycle::default(),
            projection: state::initial_projection(),
            navigation: state::Navigation::default(),
            last_state: None,
            energy_label: EnergyGaugeLabel::Fuel,
            max_lateral_accel: 3.0,
            params: None,
            debug: debug::DebugState::default(),
            route: route::Route::default(),
            media: None,
            profile: profile::Profile::default(),
        };
        if !isolated {
            match crate::media::MediaInput::new() {
                Ok(media) => input.attach_media(media),
                Err(error) => eprintln!("Carrot navigation media IPC unavailable: {error}"),
            }
            if let Ok(params) = openpilot_params::Params::for_runtime() {
                input.attach_params(
                    params,
                    openpilot_params::Params::for_runtime_at(std::path::Path::new(
                        "/dev/shm/params",
                    ))
                    .ok(),
                );
            }
        }
        Ok(input)
    }
    pub fn attach_media(&mut self, media: crate::media::MediaInput) {
        self.media = Some(media);
    }
    /// # Errors
    /// Returns native queue or Cereal validation failure.
    pub fn poll(&mut self) -> Result<(), Error> {
        let start = self.profile.start();
        self.subscriber.update(self.timeout)?;
        self.profile.add("source.live.submaster_update", start);
        Ok(())
    }
    /// # Errors
    /// Returns a malformed device event or subscription-state failure.
    pub fn onroad_state(&self) -> Result<Option<bool>, Error> {
        let topic = self.subscriber.state.topic("deviceState")?;
        if !topic.alive {
            return Ok(None);
        }
        Ok(match topic.event()?.which()? {
            event::DeviceState(data) => Some(data?.get_started()),
            _ => None,
        })
    }
    /// # Errors
    /// Returns a malformed selfdrive event or subscription-state failure.
    pub fn alert(
        &mut self,
        current: Option<ClusterAlert>,
        onroad: bool,
        now: f64,
    ) -> Result<Option<ClusterAlert>, Error> {
        let topic = self.subscriber.state.topic("selfdriveState")?;
        let received = (topic.receive_time.is_finite() && topic.receive_time > 0.0)
            .then_some(topic.receive_time);
        let enabled = match topic.event()?.which()? {
            event::SelfdriveState(data) => data?.get_enabled(),
            _ => false,
        };
        Ok(self.alerts.update(
            current,
            onroad,
            now,
            alerts::Receipt {
                received,
                updated: topic.updated,
                enabled,
            },
        ))
    }
    /// # Errors
    /// Returns a frame-count conversion failure.
    pub fn status_text(&self, now: f64, radar: usize, detected: usize) -> Result<String, Error> {
        let start = self.profile.start();
        let topics = self.subscriber.state.topics();
        let alive = topics.iter().filter(|topic| topic.alive).count();
        let updated = topics.iter().filter(|topic| topic.updated).count();
        let can = if topics.iter().any(|topic| topic.service.name == "sendcan") {
            "can/sendcan"
        } else {
            "no-can"
        };
        let age = now - self.started;
        let fps = if age > 0.1 {
            self.frames.to_f64().ok_or(Error::Clock)? / age
        } else {
            0.0
        };
        let text = format!("live {can} alive={alive}/{} upd={updated} state={fps:.1}Hz radar={radar} detected={detected}", topics.len());
        self.profile.add("source.live.status_text", start);
        Ok(text)
    }
}

/// # Errors
/// Returns a floating-point conversion failure for the native monotonic clock.
pub fn monotonic() -> Result<f64, Error> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Ok(now.tv_sec.to_f64().ok_or(Error::Clock)? + now.tv_nsec.to_f64().ok_or(Error::Clock)? / 1e9)
}
