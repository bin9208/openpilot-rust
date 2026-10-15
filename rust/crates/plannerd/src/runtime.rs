use crate::{
    carrot::CarrotPlanner, config::Config, fast_radar::FastRadarOverlay,
    lane_departure::LaneDeparture, lateral_planner::LateralPlanner,
    longitudinal_planner::LongitudinalPlanner, parameters::Parameters,
    stopping_lead::StoppingLeadFilter, Error,
};
use openpilot_logging::{producer::Logger, runtime::RuntimeDiagnostics};
use openpilot_messaging::{
    runtime::PubMaster,
    state::{Options, Poll},
};
use std::path::Path;

mod daemon;
mod inputs;
mod process;
mod signals;
mod views;
pub use daemon::run;
pub use process::Tick;
pub use signals::Inputs as SignalInputs;

pub const SERVICES: [&str; 11] = [
    "carControl",
    "carState",
    "controlsState",
    "liveParameters",
    "radarState",
    "liveTracks",
    "modelV2",
    "selfdriveState",
    "carrotMan",
    "livePose",
    "carrotNavi",
];
pub const PUBLICATIONS: [&str; 3] = ["longitudinalPlan", "driverAssistance", "lateralPlan"];

pub struct Planner {
    config: Config,
    pub longitudinal: LongitudinalPlanner,
    pub lateral: LateralPlanner,
    pub carrot: CarrotPlanner,
    fast: FastRadarOverlay,
    stopping: StoppingLeadFilter,
    departure: LaneDeparture,
    live_tracks: bool,
    model_frame: u64,
    last_longitudinal_ns: u64,
    signals: signals::RuntimeSignals,
}

impl Planner {
    pub fn load(
        config: Config,
        artifact: &Path,
        parameters: &mut impl Parameters,
    ) -> Result<Self, Error> {
        Ok(Self::load_with(config, artifact, parameters, |_| Ok(()))?.0)
    }

    pub fn load_with<T>(
        config: Config,
        artifact: &Path,
        parameters: &mut impl Parameters,
        setup: impl FnOnce(Options) -> Result<T, Error>,
    ) -> Result<(Self, T), Error> {
        let mode = parameters.integer("EnableRadarTracks")?;
        let live_tracks = config.live_tracks(mode);
        let departure = LaneDeparture::default();
        let longitudinal = LongitudinalPlanner::load(config.longitudinal, artifact, 0., 0., 0.05)?;
        let lateral = LateralPlanner::load(config.lateral, artifact, parameters)?;
        let fast = FastRadarOverlay::new(config.front_delay);
        let stopping = StoppingLeadFilter::default();
        let attached = setup(options(live_tracks))?;
        let signal_constructed_at =
            crate::platform::Clock::monotonic(&crate::platform::SystemClock);
        let signal_mode = crate::signal::transport::Mode::from_parameter(i64::from(
            parameters.integer("SignalAssistMode").unwrap_or(0),
        ));
        let carrot = CarrotPlanner::new(parameters)?;
        Ok((
            Self {
                config,
                longitudinal,
                lateral,
                carrot,
                fast,
                stopping,
                departure,
                live_tracks,
                model_frame: 0,
                last_longitudinal_ns: 0,
                signals: signals::RuntimeSignals::new_at(signal_mode, signal_constructed_at),
            },
            attached,
        ))
    }

    /// Attach owned acquisition boundaries before processing the first frame.
    pub fn with_signal_inputs(mut self, inputs: SignalInputs) -> Self {
        self.signals = self.signals.with_inputs(inputs);
        self
    }

    pub fn options(&self) -> Options {
        options(self.live_tracks)
    }
}

pub trait Output {
    fn send(&mut self, name: &'static str, bytes: &[u8]) -> Result<(), Error>;
    fn warning(&mut self, message: String) -> Result<(), Error>;
    fn info(&mut self, message: String) -> Result<(), Error>;
    fn event(&mut self, name: &str, fields: serde_json::Value) -> Result<(), Error> {
        self.info(format!("{name}: {fields}"))
    }
}

pub struct NativeOutput {
    pub publisher: PubMaster,
    pub logger: Logger,
    pub diagnostics: RuntimeDiagnostics,
}

impl Output for NativeOutput {
    fn event(&mut self, name: &str, fields: serde_json::Value) -> Result<(), Error> {
        let object = fields
            .as_object()
            .ok_or(Error::Contract("signal event fields"))?;
        let fields = object
            .iter()
            .map(|(key, value)| {
                Ok((
                    key.clone(),
                    openpilot_logging::Value::from_json(value.clone())?,
                ))
            })
            .collect::<Result<openpilot_logging::Fields, openpilot_logging::Error>>()?;
        self.logger.emit(
            openpilot_logging::log_site!(),
            openpilot_logging::record::Record::event(name, Vec::new(), fields)?,
        )?;
        Ok(())
    }
    fn send(&mut self, name: &'static str, bytes: &[u8]) -> Result<(), Error> {
        Ok(self.publisher.send(name, bytes)?)
    }
    fn warning(&mut self, message: String) -> Result<(), Error> {
        self.logger.emit(
            openpilot_logging::log_site!(),
            openpilot_logging::record::Record::text(
                openpilot_logging::record::Level::Warning,
                message,
            ),
        )?;
        Ok(())
    }
    fn info(&mut self, message: String) -> Result<(), Error> {
        self.logger.emit(
            openpilot_logging::log_site!(),
            openpilot_logging::record::Record::text(
                openpilot_logging::record::Level::Info,
                message,
            ),
        )?;
        Ok(())
    }
}

fn options(live_tracks: bool) -> Options {
    Options {
        poll: if live_tracks {
            Poll::Many(vec!["modelV2".into(), "liveTracks".into()])
        } else {
            Poll::One("modelV2".into())
        },
        ignore_frequency: vec!["radarState".into()],
        ..Options::default()
    }
}
