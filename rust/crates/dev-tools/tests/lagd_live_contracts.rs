//! Independent original-only live process phases, environment and error effects.
#![cfg(feature = "lagd-tools-native")]
use openpilot_dev_tools::{
    lagd_live_check::{scenario_with, Options, Runtime, Scenario, Spawn},
    qa_process::Captured,
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
struct Peer<'a> {
    row: &'a Value,
    call: usize,
    roles: Vec<String>,
    closed: Vec<usize>,
    prefix: String,
    params: String,
    case: &'a Path,
    sleeps: Vec<f64>,
    reads: usize,
    writes: usize,
}
impl Peer<'_> {
    fn mode(&self) -> &str {
        self.row["mode"].as_str().unwrap()
    }
    fn portable(&self, v: &str) -> String {
        v.replace(self.case.to_str().unwrap(), "@CASE@")
            .replace(&self.params, "@PARAMS@")
            .replace(&self.prefix, "@PREFIX@")
    }
}
impl Runtime for Peer<'_> {
    fn environment(&self) -> BTreeMap<OsString, OsString> {
        [
            ("ZMQ", "original-probe"),
            ("CEREAL_FAKE", "yes"),
            ("LAGD_ENV_PROBE", "retain"),
        ]
        .map(|(k, v)| (k.into(), v.into()))
        .into()
    }
    fn spawn(&mut self, spec: &Spawn) -> Result<usize> {
        let want = &self.row["calls"][self.call];
        self.call += 1;
        self.prefix = spec.env[std::ffi::OsStr::new("OPENPILOT_PREFIX")]
            .to_str()
            .unwrap()
            .to_owned();
        self.params = spec.env[std::ffi::OsStr::new("PARAMS_ROOT")]
            .to_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            json!(spec
                .argv
                .iter()
                .map(|v| self.portable(v.to_str().unwrap()))
                .collect::<Vec<_>>()),
            want["argv"]
        );
        let env = spec
            .env
            .iter()
            .filter_map(|(k, v)| {
                let k = k.to_str().unwrap();
                [
                    "PARAMS_ROOT",
                    "OPENPILOT_PREFIX",
                    "DEBUG",
                    "SIMULATION",
                    "ZMQ",
                    "CEREAL_FAKE",
                    "LAGD_ENV_PROBE",
                ]
                .contains(&k)
                .then(|| {
                    (
                        k.to_owned(),
                        Value::String(self.portable(v.to_str().unwrap())),
                    )
                })
            })
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(Value::Object(env), want["env"]);
        assert_eq!(spec.stdin_pipe, want["stdin"] == json!(-1));
        assert_eq!(
            spec.new_session,
            want["start_new_session"].as_bool().unwrap()
        );
        for (name, path) in [("stdout", &spec.stdout), ("stderr", &spec.stderr)] {
            let got = path
                .as_ref()
                .map(|v| json!(self.portable(v.to_str().unwrap())))
                .unwrap_or(json!(-1));
            assert_eq!(got, want[name]);
        }
        let role = if self.row["kind"] == "daemon" && self.call == 1 {
            "peer"
        } else {
            "native"
        };
        if self.mode() == "spawn-error"
            || self.mode() == "peer-spawn-error"
            || (self.mode() == "native-spawn-error" && role == "native")
        {
            return Err("captured spawn failure".into());
        }
        let id = self.roles.len();
        self.roles.push(role.into());
        Ok(id)
    }
    fn read_line(&mut self, _id: usize, timeout: Option<Duration>) -> Result<Vec<u8>> {
        self.reads += 1;
        if self.reads == 1 {
            assert!(timeout.is_none());
            return Ok(match self.mode() {
                "peer-eof" => b"".to_vec(),
                "peer-bad-ready" => b"not-ready\n".to_vec(),
                _ => b"ready\n".to_vec(),
            });
        }
        assert_eq!(timeout, Some(Duration::from_secs(6)));
        if self.mode() == "response-timeout" {
            return Err("captured peer response timeout".into());
        }
        Ok(match self.mode() {
            "response-json" => b"not-json\n".to_vec(),
            "response-null" => b"{\"packet\":null}\n".to_vec(),
            _ => b"{\"packet\":[256]}\n".to_vec(),
        })
    }
    fn write_flush(&mut self, _id: usize, bytes: &[u8]) -> Result<()> {
        let want = self.row["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["event"] == "write")
            .nth(self.writes)
            .unwrap();
        self.writes += 1;
        assert_eq!(
            std::str::from_utf8(bytes).unwrap(),
            want["text"].as_str().unwrap()
        );
        Ok(())
    }
    fn poll(&mut self, _id: usize) -> Result<Option<i32>> {
        Ok(match self.mode() {
            "exit-zero" => Some(0),
            "exit-seven" | "native-early-exit" => Some(7),
            _ => None,
        })
    }
    fn signal(&mut self, _id: usize, signal: i32) -> Result<()> {
        assert_eq!(signal, 15);
        Ok(())
    }
    fn wait(&mut self, _id: usize, timeout: Duration) -> Result<i32> {
        assert_eq!(timeout, Duration::from_secs(3));
        Ok(0)
    }
    fn communicate(&mut self, _id: usize, timeout: Duration) -> Result<Captured> {
        assert_eq!(timeout, Duration::from_secs(3));
        if self.mode() == "wait-timeout" {
            return Err("captured communicate timeout".into());
        }
        Ok(Captured {
            code: if self.mode() == "exit-seven" { 7 } else { 0 },
            stdout: if self.mode() == "bad-utf8" {
                vec![255]
            } else if self.mode() == "raw-lines" {
                b"out\r\nmiddle\rend\n".to_vec()
            } else {
                b"captured-out\n".to_vec()
            },
            stderr: if self.mode() == "raw-lines" {
                b"err\r\n".to_vec()
            } else {
                b"captured-err\n".to_vec()
            },
        })
    }
    fn executable_is(&mut self, _id: usize, _path: &Path) -> Result<bool> {
        Ok(true)
    }
    fn close(&mut self, id: usize) -> Result<()> {
        assert!(!self.closed.contains(&id));
        self.closed.push(id);
        Ok(())
    }
    fn sleep(&mut self, time: Duration) -> Result<()> {
        self.sleeps.push(time.as_secs_f64());
        Ok(())
    }
    fn monotonic(&mut self) -> Result<f64> {
        Ok(200.)
    }
}
fn files(root: &Path, at: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for entry in fs::read_dir(at).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(files(root, &path));
        } else {
            use base64::Engine;
            out.insert(
                path.strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
                base64::engine::general_purpose::STANDARD.encode(fs::read(path).unwrap()),
            );
        }
    }
    out
}
fn compare_original(raw: &[u8], digest: &str, count: usize) {
    assert_eq!(format!("{:x}", Sha256::digest(raw)), digest);
    let original: Value = serde_json::from_slice(raw).unwrap();
    assert_eq!(original["cases"].as_array().unwrap().len(), count);
    for row in original["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        let options = Options {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            binary: "/virtual/native".into(),
            peer: Some("/virtual/peer".into()),
            output: output.clone(),
        };
        let kind = match row["kind"].as_str().unwrap() {
            "daemon" => Scenario::Learning,
            "wait" => Scenario::WaitSignal,
            "CarParams" => Scenario::ParamsCar,
            _ => Scenario::ParamsDelay,
        };
        let mut peer = Peer {
            row,
            call: 0,
            roles: Vec::new(),
            closed: Vec::new(),
            prefix: String::new(),
            params: String::new(),
            case: temp.path(),
            sleeps: Vec::new(),
            reads: 0,
            writes: 0,
        };
        let mut stdout = Vec::new();
        let result = scenario_with(kind, &options, &mut peer, &mut stdout);
        assert_eq!(
            result.is_ok(),
            row["success"].as_bool().unwrap(),
            "{} {}: {:?}",
            row["kind"],
            row["mode"],
            result.as_ref().err().map(ToString::to_string)
        );
        if !row["result"].is_null() {
            let got: Value = serde_json::from_str(&result.unwrap().to_json().unwrap()).unwrap();
            assert_eq!(got, row["result"]);
        }
        assert_eq!(
            String::from_utf8(stdout).unwrap(),
            row["stdout"].as_str().unwrap()
        );
        assert_eq!(peer.call, row["calls"].as_array().unwrap().len());
        assert_eq!(peer.closed.len(), peer.roles.len());
        let want_sleeps = row["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["event"] == "sleep")
            .map(|v| v["seconds"].as_f64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(peer.sleeps, want_sleeps);
        assert_eq!(
            serde_json::to_value(files(temp.path(), temp.path())).unwrap(),
            row["files"],
            "{} {} files",
            row["kind"],
            row["mode"]
        );
        if !peer.prefix.is_empty() {
            assert!(!PathBuf::from(format!("/dev/shm/msgq_{}",peer.prefix)).exists(),"only exact owned namespace must be removed, including original spawn-before-finally leak");
        }
    }
}

#[test]
fn original_24_process_environment_deadline_and_cleanup_boundaries() {
    compare_original(
        include_bytes!("fixtures/lagd-tools/live-boundaries-original.json"),
        "a22a652db7e23c2365eb5cdee8177e1f7017ad57a67ea7246fb366bcf0c6183a",
        24,
    );
}
#[test]
fn original_3_successful_wait_signal_and_params_communication_reports() {
    compare_original(
        include_bytes!("fixtures/lagd-tools/live-healthy-original.json"),
        "37959a4ca6349f28b7d95b08c5457badfa5d35ba1d0e5cf225af2e3ea1818bf5",
        3,
    );
}

#[test]
fn original_3_binary_stdio_preserves_crlf_and_bare_cr_without_text_translation() {
    compare_original(
        include_bytes!("fixtures/lagd-tools/live-raw-lines-original.json"),
        "9f015b8284c38e01d4820790995e6f6887cffcd3fe2a25bbe4b716d6bf1c01e3",
        3,
    );
}
