//! Bounded read-only observation transport; cached JSON never caches freshness.
use serde_json::Value;
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};
pub const MAX_BYTES: usize = 32768;
pub const START_PATH: &str = "/dev/shm/carrot_signal_start_observation.json";
pub const PREPARE_PATH: &str = "/dev/shm/carrot_signal_prepare_observation.json";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    Observe,
    Prepare,
}
impl Mode {
    pub fn from_parameter(value: i64) -> Self {
        match value {
            1 => Self::Observe,
            2 => Self::Prepare,
            _ => Self::Off,
        }
    }
    pub fn enabled(self) -> bool {
        self != Self::Off
    }
    pub fn actuate(self) -> bool {
        self == Self::Prepare
    }
}
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
pub struct Service {
    pub seen: bool,
    pub valid: bool,
    pub alive: bool,
    pub receive_time: f64,
}
impl Service {
    pub fn fresh(self, now: f64, max_age: f64) -> bool {
        self.seen
            && self.valid
            && self.alive
            && (0. ..=max_age).contains(&(now - self.receive_time))
    }
}
#[derive(Debug, Default, serde::Deserialize)]
pub struct VehicleContext {
    /// carState, modelV2, radarState, selfdriveState and carControl.
    pub services: [Service; 5],
    pub can_valid: bool,
    pub selfdrive_enabled: bool,
    pub longitudinal_active: bool,
    pub drive: bool,
}
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Context {
    pub now: f64,
    pub enabled: bool,
    pub valid: bool,
    pub drive: bool,
}
#[derive(Debug)]
pub struct Reader {
    enabled: bool,
    last_mode_check: f64,
    enabled_since: f64,
    bytes: Option<Vec<u8>>,
    observation: Option<Value>,
}
impl Reader {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn snapshot(&self) -> Value {
        serde_json::json!({"enabled":self.enabled,"last_mode_check":self.last_mode_check,"enabled_since":self.enabled_since,"bytes":self.bytes,"observation":self.observation})
    }
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last_mode_check: -1.,
            enabled_since: f64::NEG_INFINITY,
            bytes: None,
            observation: None,
        }
    }
    pub fn read(
        &mut self,
        now: f64,
        requested: impl FnOnce() -> bool,
        path: &Path,
        vehicle: &VehicleContext,
    ) -> (Option<Value>, Context) {
        self.read_with(
            now,
            requested,
            || {
                let mut bytes = Vec::new();
                File::open(path)?
                    .take((MAX_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)?;
                Ok(bytes)
            },
            vehicle,
        )
    }
    pub fn read_with(
        &mut self,
        now: f64,
        requested: impl FnOnce() -> bool,
        load: impl FnOnce() -> io::Result<Vec<u8>>,
        vehicle: &VehicleContext,
    ) -> (Option<Value>, Context) {
        if now - self.last_mode_check >= 0.5 {
            let enabled = requested();
            if enabled && !self.enabled {
                self.enabled_since = now;
            }
            self.enabled = enabled;
            self.last_mode_check = now;
        }
        let mut observation = None;
        if self.enabled {
            match load() {
                Ok(bytes) if bytes.len() <= MAX_BYTES => {
                    if self.bytes.as_ref() != Some(&bytes) {
                        match serde_json::from_slice::<Value>(&bytes) {
                            Ok(value) => {
                                self.observation = Some(value);
                                self.bytes = Some(bytes);
                            }
                            Err(_) => {
                                self.bytes = None;
                                self.observation = None;
                            }
                        }
                    }
                    if let Some(obj) = &self.observation {
                        match envelope(obj, self.enabled_since) {
                            Ok(true) => observation = Some(obj.clone()),
                            Ok(false) => (),
                            Err(()) => {
                                self.bytes = None;
                                self.observation = None;
                            }
                        }
                    }
                }
                _ => {
                    self.bytes = None;
                    self.observation = None;
                }
            }
        } else {
            self.bytes = None;
            self.observation = None;
        }
        let context = Context {
            now,
            enabled: self.enabled && vehicle.selfdrive_enabled && vehicle.longitudinal_active,
            valid: self.enabled
                && vehicle.can_valid
                && vehicle.services.iter().all(|s| s.fresh(now, 0.25)),
            drive: self.enabled && vehicle.drive,
        };
        (observation, context)
    }
}

pub(crate) fn numeric(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_bool().map(|v| f64::from(u8::from(v))))
}
fn envelope(value: &Value, enabled_since: f64) -> Result<bool, ()> {
    let obj = value.as_object().ok_or(())?;
    if obj.get("version").and_then(numeric) != Some(1.)
        || obj.get("stream").and_then(Value::as_str) != Some("road")
        || !obj
            .get("size")
            .and_then(Value::as_array)
            .is_some_and(|size| {
                size.len() == 2
                    && numeric(&size[0]) == Some(1344.)
                    && numeric(&size[1]) == Some(760.)
            })
    {
        return Ok(false);
    }
    let timestamp = match obj.get("timestamp") {
        Some(value) => numeric(value).ok_or(())?,
        None => f64::NEG_INFINITY,
    };
    Ok(timestamp >= enabled_since)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn vehicle(now: f64) -> VehicleContext {
        VehicleContext {
            services: [Service {
                seen: true,
                valid: true,
                alive: true,
                receive_time: now,
            }; 5],
            can_valid: true,
            selfdrive_enabled: true,
            longitudinal_active: true,
            drive: true,
        }
    }
    #[test]
    fn cached_bytes_recheck_context_and_new_enable_rejects_old_capture() {
        let mut reader = Reader::new(false);
        let packet = serde_json::to_vec(
            &serde_json::json!({"version":1,"stream":"road","size":[1344,760],"timestamp":10.}),
        )
        .unwrap();
        let read = |r: &mut Reader, now, enabled, v: &VehicleContext| {
            r.read_with(now, || enabled, || Ok(packet.clone()), v)
        };
        assert!(read(&mut reader, 10., true, &vehicle(10.)).0.is_some());
        assert!(!read(&mut reader, 10.3, true, &vehicle(10.)).1.valid);
        assert!(read(&mut reader, 10.6, false, &vehicle(10.6)).0.is_none());
        assert!(read(&mut reader, 11.2, true, &vehicle(11.2)).0.is_none());
    }
    #[test]
    fn oversized_or_malformed_packet_cannot_reuse_old_cached_observation() {
        let mut reader = Reader::new(true);
        let (obs, _) = reader.read_with(
            10.,
            || true,
            || Ok(vec![b' '; MAX_BYTES + 1]),
            &vehicle(10.),
        );
        assert!(obs.is_none());
        let (obs, _) = reader.read_with(10.1, || true, || Ok(b"{".to_vec()), &vehicle(10.1));
        assert!(obs.is_none());
    }

    #[test]
    fn disabled_setting_uses_source_early_return_context() {
        let (_, context) =
            Reader::new(false).read_with(10., || false, || Ok(Vec::new()), &vehicle(10.));
        assert!(!context.enabled);
        assert!(!context.valid);
        assert!(!context.drive);
    }
}

/// Shared half-second production setting cache, matching source flag overrides.
#[derive(Debug)]
pub struct ModeCache {
    mode: Mode,
    next_read: f64,
}
impl Default for ModeCache {
    fn default() -> Self {
        Self {
            mode: Mode::Off,
            next_read: 0.,
        }
    }
}
impl ModeCache {
    /// Constructor request primes the same cache subsequently shared by both readers.
    pub fn primed(mode: Mode, now: f64) -> Self {
        Self {
            mode,
            next_read: now + 0.5,
        }
    }
    pub fn read<E>(&mut self, now: f64, read: impl FnOnce() -> Result<i64, E>) -> Mode {
        if now >= self.next_read {
            self.mode = Mode::from_parameter(read().unwrap_or(0));
            self.next_read = now + 0.5;
        }
        self.mode
    }
}

pub fn production_flag(path: &Path, mode: Mode) -> Option<bool> {
    if path.parent() != Some(Path::new("/data/signal-color-shadow")) {
        return None;
    }
    match path.file_name()?.to_str()? {
        "assist_enabled" => Some(false),
        "enabled"
        | "tracking_enabled"
        | "display_enabled"
        | "start_guard_enabled"
        | "stop_prepare_enabled" => Some(mode.enabled()),
        _ => None,
    }
}
#[cfg(test)]
mod mode_tests {
    use super::*;
    #[test]
    fn construction_primes_half_second_cache_before_first_planner_phase() {
        let mut cache = ModeCache::primed(Mode::Observe, 100.);
        let mut reads = 0;
        assert_eq!(
            cache.read(100.2, || {
                reads += 1;
                Ok::<_, ()>(2)
            }),
            Mode::Observe
        );
        assert_eq!(reads, 0);
        assert_eq!(
            cache.read(100.5, || {
                reads += 1;
                Ok::<_, ()>(2)
            }),
            Mode::Prepare
        );
        assert_eq!(reads, 1);
    }

    #[test]
    fn both_consumers_share_cached_mode_and_errors_disable_at_next_read() {
        let mut cache = ModeCache::default();
        let mut reads = 0;
        assert_eq!(
            cache.read(1., || {
                reads += 1;
                Ok::<_, ()>(2)
            }),
            Mode::Prepare
        );
        assert_eq!(
            cache.read(1.2, || {
                reads += 1;
                Err(())
            }),
            Mode::Prepare
        );
        assert_eq!(reads, 1);
        assert_eq!(
            cache.read(1.5, || {
                reads += 1;
                Err(())
            }),
            Mode::Off
        );
        assert_eq!(reads, 2);
        assert_eq!(
            production_flag(
                Path::new("/data/signal-color-shadow/assist_enabled"),
                Mode::Prepare
            ),
            Some(false)
        );
        assert_eq!(
            production_flag(Path::new("/tmp/assist_enabled"), Mode::Prepare),
            None
        );
    }
}
