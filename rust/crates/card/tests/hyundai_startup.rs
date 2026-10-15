use openpilot_can::Frame;
use openpilot_card::{
    brands::hyundai::{Hyundai, Setup},
    firmware_query::StartupIo,
    isotp::Error,
    query::{DiagnosticLevel, QueryIo},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
};

#[derive(Deserialize)]
struct Case {
    params: PathBuf,
    settings: BTreeMap<String, i32>,
    fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
    response: bool,
    expected: Transcript,
}
#[derive(Default, Deserialize, Serialize)]
struct Transcript {
    logs: Vec<(String, String)>,
    printed: Vec<String>,
    sent: Vec<Frame>,
    receives: Vec<bool>,
    delays: Vec<f64>,
    now: f64,
    personality: i32,
    radar_result: Option<i32>,
}
#[derive(Default)]
struct Io {
    pending: VecDeque<Vec<Vec<Frame>>>,
    response: bool,
    output: Transcript,
}

impl QueryIo for Io {
    fn log(&mut self, level: DiagnosticLevel, message: &str) {
        let name = match level {
            DiagnosticLevel::Info => "info",
            DiagnosticLevel::Warning => "warning",
            DiagnosticLevel::Error => "error",
            DiagnosticLevel::Exception => "exception",
        };
        self.output.logs.push((name.into(), message.into()));
    }
    fn receive(&mut self, wait: bool) -> Result<Vec<Vec<Frame>>, Error> {
        self.output.receives.push(wait);
        Ok(self.pending.pop_front().unwrap_or_default())
    }
    fn send(&mut self, frames: &[Frame]) -> Result<(), Error> {
        self.output.sent.extend_from_slice(frames);
        if self.response {
            for frame in frames {
                let data = &frame.data;
                let reply = if data[0] == 2 && data[1] == 0x10 {
                    Some(vec![2, 0x50, data[2], 0, 0, 0, 0, 0])
                } else if data[0] & 0xf0 == 0x10 {
                    Some(vec![0x30, 0, 0, 0, 0, 0, 0, 0])
                } else {
                    None
                };
                if let Some(data) = reply {
                    self.pending.push_back(vec![vec![Frame {
                        address: frame.address + 8,
                        data,
                        bus: frame.bus,
                    }]]);
                }
            }
        }
        Ok(())
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        self.output.delays.push(seconds);
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.output.now += 0.001;
        self.output.now
    }
}
impl StartupIo for Io {
    fn set_obd_multiplexing(&mut self, _enabled: bool) -> Result<(), Error> {
        panic!("Hyundai init must not change OBD multiplexing")
    }
}

#[test]
fn startup_when_disable_blinker_radar_success_timeout_and_deinit() {
    let root = PathBuf::from(std::env::var("HYUNDAI_FIXTURE_DIR").unwrap());
    let dbc = PathBuf::from(std::env::var("HYUNDAI_DBC_DIR").unwrap());
    let cases: Vec<Case> =
        serde_json::from_slice(&std::fs::read(root.join("startup.json")).unwrap()).unwrap();
    let mut outputs = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let params =
            openpilot_params::Params::open(&root.join(format!("startup-native-{index}")), "p")
                .unwrap();
        for (key, value) in &case.settings {
            if key != "EnableRadarTracksResult" {
                params.put(key, value.to_string().as_bytes()).unwrap();
            }
        }
        let bytes = std::fs::read(&case.params).unwrap();
        let mut vehicle = Hyundai::new(Setup {
            params_bytes: &bytes,
            dbc_root: &dbc,
            settings: params,
            fingerprints: &case.fingerprints,
            now_ns: 2_000_000_000,
        })
        .unwrap();
        let mut io = Io {
            response: case.response,
            ..Io::default()
        };
        vehicle.take_diagnostics();
        vehicle.init(&mut io).unwrap();
        io.output.printed = vehicle.take_diagnostics();
        io.output.personality = std::str::from_utf8(
            &vehicle
                .state
                .settings
                .get("LongitudinalPersonalityMax")
                .unwrap()
                .unwrap(),
        )
        .unwrap()
        .parse()
        .unwrap();
        io.output.radar_result = vehicle
            .state
            .settings
            .get("EnableRadarTracksResult")
            .unwrap()
            .map(|value| std::str::from_utf8(&value).unwrap().parse().unwrap());
        let before = serde_json::to_value(&io.output).unwrap();
        vehicle.deinit(&mut io).unwrap();
        assert_eq!(serde_json::to_value(&io.output).unwrap(), before);
        assert_eq!(
            serde_json::to_value(&io.output).unwrap(),
            serde_json::to_value(&case.expected).unwrap(),
            "case {index}"
        );
        outputs.push(io.output);
    }
    std::fs::write(
        root.join("native-startup.json"),
        serde_json::to_vec(&outputs).unwrap(),
    )
    .unwrap();
}

#[test]
fn final_bosch_passive_startup_preserves_source_ordered_ecu_effects_and_params() {
    use openpilot_card::brands::hyundai::{bus::CanBus, flags};
    use serde_json::{json, Value};
    let dbc = PathBuf::from(std::env::var("HYUNDAI_DBC_DIR").unwrap());
    let params_root = tempfile::tempdir().unwrap();
    let params = openpilot_params::Params::open(params_root.path(), "p").unwrap();
    let mut message = openpilot_card::vehicle_params::baseline("HYUNDAI_ELANTRA").unwrap();
    message
        .get_root::<openpilot_cereal::car_capnp::car_params::Builder<'_>>()
        .unwrap()
        .init_safety_configs(1);
    let bytes = capnp::serialize::write_message_to_words(&message);
    let mut vehicle = Hyundai::new(Setup {
        params_bytes: &bytes,
        dbc_root: &dbc,
        settings: params,
        fingerprints: &[],
        now_ns: 2_000_000_000,
    })
    .unwrap();
    let data = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-startup-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&data).unwrap();
    assert_eq!(rows.len(), 512);
    for (index, row) in rows.iter().enumerate() {
        let config = &mut vehicle.state.config;
        config.flags = row["flags"].as_u64().unwrap() as u32;
        config.ext_flags = row["ext_flags"].as_u64().unwrap() as u32;
        config.longitudinal = row["longitudinal"].as_bool().unwrap();
        config.bus = CanBus::with_offset(
            row["offset"].as_u64().unwrap() as u8,
            config.flags & flags::HDA2 != 0,
            row["camera_scc"].as_i64().unwrap() as i32,
        );
        for (key, value) in [
            ("EnableRadarTracks", row["radar_tracks"].as_i64().unwrap()),
            ("HyundaiCameraSCC", row["camera_scc"].as_i64().unwrap()),
            ("LongitudinalPersonalityMax", -1),
            ("EnableRadarTracksResult", -1),
        ] {
            vehicle
                .state
                .settings
                .put(key, value.to_string().as_bytes())
                .unwrap();
        }
        let mut io = Io {
            response: true,
            ..Io::default()
        };
        vehicle.init(&mut io).unwrap();
        let actual: Vec<Value> = io
            .output
            .sent
            .iter()
            .filter_map(|frame| {
                if frame.data.starts_with(&[3, 0x28, 0x83, 1]) {
                    Some(json!([
                        "disable",
                        frame.address,
                        frame.bus,
                        [0x28, 0x83, 1]
                    ]))
                } else if frame.data.starts_with(&[2, 0x10, 7]) {
                    Some(json!(["enable"]))
                } else {
                    None
                }
            })
            .collect();
        let expected: Vec<Value> = row["actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|action| action[0] == "disable" || action[0] == "enable")
            .cloned()
            .collect();
        assert_eq!(actual, expected, "ordered ECU effects/{index}");
        for key in ["LongitudinalPersonalityMax", "EnableRadarTracksResult"] {
            let expected = row["actions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|action| action[1] == key)
                .map_or(-1, |action| {
                    action[2]
                        .as_i64()
                        .unwrap_or_else(|| i64::from(action[2].as_bool().unwrap()))
                });
            let actual = vehicle.state.settings.get(key).unwrap().unwrap();
            let actual: i64 = std::str::from_utf8(&actual).unwrap().parse().unwrap();
            assert_eq!(actual, expected, "durable {key}/{index}");
        }
        vehicle.take_diagnostics();
    }
}
