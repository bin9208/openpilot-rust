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
