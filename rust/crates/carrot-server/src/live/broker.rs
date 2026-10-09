use super::snapshot::snapshot;
use crate::{params::Backend, Error, Value};
use openpilot_messaging::{runtime::SubMaster, state::Options};
use openpilot_params::Params;
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

const SERVICES: [&str; 3] = ["selfdriveState", "navInstructionCarrot", "navRoute"];
const PARAMS: [&str; 15] = [
    "IsMetric",
    "LongitudinalPersonality",
    "ShowDateTime",
    "ShowPathEnd",
    "ShowLaneInfo",
    "ShowPathMode",
    "ShowPathColor",
    "ShowPathModeLane",
    "ShowPathColorLane",
    "ShowPathColorCruiseOff",
    "ShowPathWidth",
    "ShowPlotMode",
    "ShowRadarInfo",
    "RadarLatFactor",
    "CustomSR",
];
pub(super) struct Broker {
    sm: SubMaster,
    params: Option<Backend>,
    snapshot: Value,
    last_poll: Option<Instant>,
    engaged: watch::Sender<bool>,
}
pub(super) fn wall_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn meta(now: u128) -> Value {
    Value::object([
        ("schemaVersion", Value::integer(2)),
        ("repoFlavor", Value::text("c3")),
        ("selectedCamera", Value::text("road")),
        ("generatedAtMs", Value::integer(now)),
        ("payloadKind", Value::text("service-raw-v1")),
    ])
}
impl Broker {
    pub fn new(params: Option<Params>, engaged: watch::Sender<bool>) -> Result<Self, Error> {
        let sm = SubMaster::for_runtime(&SERVICES, Options::default())
            .map_err(|error| Error::Source(error.to_string()))?;
        Ok(Self {
            sm,
            params: params.map(|params| Backend::native(params, PathBuf::new())),
            snapshot: Value::object([
                ("meta", meta(wall_ms())),
                ("runtime", Value::object([])),
                ("services", Value::object([])),
            ]),
            last_poll: None,
            engaged,
        })
    }
    fn age(&self) -> Value {
        self.last_poll.map_or(Value::Null, |time| {
            Value::integer(time.elapsed().as_millis())
        })
    }
    pub fn response(&mut self, force: bool) -> Result<Value, Error> {
        let due = self.last_poll.is_none_or(|time| {
            time.elapsed().as_millis() > 4800 || (force && time.elapsed().as_millis() > 100)
        });
        if due || !self.snapshot.get("runtime").get("params").truth() {
            self.poll()?;
        }
        let all = self.snapshot.get("services");
        let services = Value::Object(
            ["navInstructionCarrot", "navRoute"]
                .into_iter()
                .filter_map(|name| {
                    let value = all.get(name);
                    matches!(value, Value::Object(_))
                        .then(|| (name.chars().map(u32::from).collect(), value.clone()))
                })
                .collect(),
        );
        Ok(Value::object([
            ("ok", Value::Bool(true)),
            ("meta", self.snapshot.get("meta").clone()),
            ("runtime", self.snapshot.get("runtime").clone()),
            ("services", services),
            ("snapshotAgeMs", self.age()),
        ]))
    }
    fn poll(&mut self) -> Result<(), Error> {
        self.sm
            .update(Duration::ZERO)
            .map_err(|error| Error::Source(error.to_string()))?;
        let now = wall_ms();
        let mut meta = meta(now);
        crate::json_fields::set(
            &mut meta,
            "capabilities",
            Value::Array(
                [
                    "live",
                    "camera-road",
                    "road-only-ui",
                    "carrotlink-projection",
                ]
                .map(Value::text)
                .into(),
            ),
        )?;
        let mut services = Vec::new();
        let mut alive = Vec::new();
        let mut core = Vec::new();
        let mut optional = Vec::new();
        let mut missing = Vec::new();
        let mut missing_optional = Vec::new();
        let mut enabled = false;
        for topic in self.sm.state.topics() {
            let name = topic.service.name;
            let key: Vec<_> = name.chars().map(u32::from).collect();
            alive.push((key.clone(), Value::Bool(topic.alive)));
            if name == "selfdriveState" {
                core.push((key.clone(), Value::Bool(topic.alive)));
            } else {
                optional.push((key.clone(), Value::Bool(topic.alive)));
            }
            if !topic.alive {
                missing.push(Value::text(name));
                if name != "selfdriveState" {
                    missing_optional.push(Value::text(name));
                }
            }
            let payload = if topic.alive {
                topic
                    .data()
                    .map(|data| snapshot(name, data))
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            if name == "selfdriveState" {
                enabled = payload.get("enabled").truth();
            }
            services.push((key, payload));
        }
        self.engaged.send_replace(enabled);
        let core_alive = core.iter().filter(|(_, value)| value.truth()).count();
        let optional_alive = optional.iter().filter(|(_, value)| value.truth()).count();
        let runtime = Value::object([
            ("generatedAtMs", Value::integer(now)),
            ("snapshotFresh", Value::Bool(core_alive == 1)),
            ("serviceAlive", Value::Object(alive)),
            ("coreServicesAlive", Value::Object(core)),
            ("optionalServicesAlive", Value::Object(optional)),
            ("activeCoreServices", Value::integer(core_alive)),
            ("activeOptionalServices", Value::integer(optional_alive)),
            ("missingServices", Value::Array(missing)),
            (
                "missingCoreServices",
                Value::Array(if core_alive == 0 {
                    vec![Value::text("selfdriveState")]
                } else {
                    Vec::new()
                }),
            ),
            ("missingOptionalServices", Value::Array(missing_optional)),
            ("params", self.read_params()),
        ]);
        self.snapshot = Value::object([
            ("meta", meta),
            ("runtime", runtime),
            ("services", Value::Object(services)),
        ]);
        self.last_poll = Some(Instant::now());
        Ok(())
    }
    fn read_params(&self) -> Value {
        let Some(params) = &self.params else {
            return Value::object([]);
        };
        Value::Object(
            PARAMS
                .into_iter()
                .map(|name| {
                    let value = if name == "IsMetric" {
                        Value::text(if params.get(name, &Value::Null).truth() {
                            "1"
                        } else {
                            "0"
                        })
                    } else {
                        params
                            .typed_value(name, false)
                            .and_then(|value| value.py_string().ok())
                            .unwrap_or(Value::Null)
                    };
                    (name.chars().map(u32::from).collect(), value)
                })
                .collect(),
        )
    }
}
