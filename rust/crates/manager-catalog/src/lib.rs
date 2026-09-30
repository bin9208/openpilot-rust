//! Ordered manager catalog policy ported from system/manager/process_config.py.
//! Source launch metadata is provenance, not executable fallback configuration.
mod catalog;
pub use catalog::{catalog, Descriptor, ImportConfig, RustAvailability, SourceProcess};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("fatal Cython get_int conversion for {key}: {source}")]
    Integer {
        key: String,
        source: openpilot_beepd::IntegerError,
    },
    #[error("parameter exception: {0}")]
    Parameter(#[from] openpilot_params::Error),
}

/// Integer conversion errors are fatal at the original Cython boundary, even
/// within a Python `except Exception` block. Callers must not default them.
pub trait Parameters {
    fn get_bool(&mut self, key: &str) -> Result<bool, Error>;
    fn get_int(&mut self, key: &str) -> Result<i32, Error>;
    fn get_bool_default(&mut self, key: &str) -> Result<bool, Error>;
    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), Error>;
}

fn raw(params: &openpilot_params::Params, key: &str) -> Result<Vec<u8>, Error> {
    match params.get(key) {
        Ok(value) => Ok(value.unwrap_or_default()),
        // The source C++ read_file returns empty on filesystem read errors.
        Err(openpilot_params::Error::Io(_)) => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

impl Parameters for openpilot_params::Params {
    fn get_bool(&mut self, key: &str) -> Result<bool, Error> {
        Ok(raw(self, key)? == b"1")
    }
    fn get_int(&mut self, key: &str) -> Result<i32, Error> {
        openpilot_beepd::integer(&raw(self, key)?).map_err(|source| Error::Integer {
            key: key.to_owned(),
            source,
        })
    }
    fn get_bool_default(&mut self, key: &str) -> Result<bool, Error> {
        let value = raw(self, key)?;
        if value.is_empty() {
            Ok(openpilot_params::metadata(key).and_then(|info| info.default) == Some("1"))
        } else {
            Ok(value == b"1")
        }
    }
    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), Error> {
        match openpilot_params::Params::put_bool(self, key, value) {
            // Cython ignores the C++ putBool return code.
            Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

pub trait GpsPaths {
    fn exists(&mut self, path: &str) -> bool;
}
pub struct SystemGpsPaths;
impl GpsPaths for SystemGpsPaths {
    fn exists(&mut self, path: &str) -> bool {
        std::path::Path::new(path).exists()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct State {
    pub started: bool,
    pub not_car: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Predicate {
    DriverView,
    NotCar,
    IsCar,
    Logging,
    Ublox,
    Joystick,
    NotJoystick,
    LongManeuver,
    LatManeuver,
    NotLongManeuver,
    QcomGps,
    Always,
    Onroad,
    Offroad,
    Updated,
    DriverMonitoring,
    ShareData,
    WebRtc,
    C3xLite,
    YoutubeLow,
    YoutubeMedium,
    Youtube,
    YoutubeWide,
    ClusterHud,
    All(&'static [Predicate]),
    Any(&'static [Predicate]),
}

fn caught(result: Result<bool, Error>) -> Result<bool, Error> {
    match result {
        Err(Error::Parameter(_)) => Ok(false),
        other => other,
    }
}
fn ublox_available(paths: &mut impl GpsPaths) -> bool {
    paths.exists("/dev/ttyHS0") && !paths.exists("/persist/comma/use-quectel-gps")
}

impl Predicate {
    /// Evaluates the callback only. The manager owns enabled/not_run filtering.
    pub fn evaluate(
        self,
        state: State,
        params: &mut impl Parameters,
        paths: &mut impl GpsPaths,
    ) -> Result<bool, Error> {
        use Predicate::*;
        let State { started, not_car } = state;
        Ok(match self {
            DriverView => started || params.get_bool("IsDriverViewEnabled")?,
            NotCar => started && not_car,
            IsCar => started && !not_car,
            Logging => {
                let run = !not_car || !params.get_bool("DisableLogging")?;
                started && run
            }
            Ublox => {
                let available = ublox_available(paths);
                if available != params.get_bool("UbloxAvailable")? {
                    params.put_bool("UbloxAvailable", available)?;
                }
                started && available
            }
            Joystick => started && params.get_bool("JoystickDebugMode")?,
            NotJoystick => started && !params.get_bool("JoystickDebugMode")?,
            LongManeuver => started && params.get_bool("LongitudinalManeuverMode")?,
            LatManeuver => started && params.get_bool("LateralManeuverMode")?,
            NotLongManeuver => started && !params.get_bool("LongitudinalManeuverMode")?,
            QcomGps => started && !ublox_available(paths),
            Always => true,
            Onroad => started,
            Offroad => !started,
            Updated => !started && params.get_bool("SoftwareMenu")?,
            DriverMonitoring => {
                (started || params.get_bool("IsDriverViewEnabled")?)
                    && params.get_int("DisableDM")? == 0
            }
            ShareData => params.get_bool("ShareData")?,
            WebRtc => {
                params.get_int("DisableDM")? == 2
                    && !caught(params.get_int("ClusterHud").map(|value| value == 1))?
            }
            C3xLite => started && params.get_bool("HardwareC3xLite")?,
            ClusterHud => caught(params.get_int("ClusterHud").map(|value| value == 1))?,
            YoutubeLow | YoutubeMedium | Youtube | YoutubeWide => caught((|| {
                if self == YoutubeWide && !params.get_bool_default("UseWideCamera")? {
                    return Ok(false);
                }
                if params.get_int("CarrotYouTubeLive")? <= 0 {
                    return Ok(false);
                }
                let quality = params.get_int("CarrotYouTubeQuality")?;
                Ok(match self {
                    YoutubeLow => !matches!(quality, 1..=3),
                    YoutubeMedium => quality == 1,
                    Youtube => quality == 2,
                    YoutubeWide => quality == 3,
                    _ => unreachable!(),
                })
            })())?,
            All(predicates) => {
                for predicate in predicates {
                    if !predicate.evaluate(state, params, paths)? {
                        return Ok(false);
                    }
                }
                true
            }
            Any(predicates) => {
                for predicate in predicates {
                    if predicate.evaluate(state, params, paths)? {
                        return Ok(true);
                    }
                }
                false
            }
        })
    }
}
