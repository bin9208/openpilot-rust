//! All independently captured original checker boundaries and partial artifacts.
#![cfg(feature = "image-array-native")]
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_application_lifecycle_check::{check, Backend, Options},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
struct Peer {
    case: Value,
    output: PathBuf,
    binary: PathBuf,
    root: PathBuf,
    reference: PathBuf,
    calls: Vec<Value>,
    xops: Vec<Value>,
    packets: VecDeque<Vec<u8>>,
    code: Option<i32>,
    clock: f64,
}
impl Peer {
    fn name(&self) -> &str {
        self.case["name"].as_str().unwrap()
    }
    fn author(&self, image: &Path) -> Result<()> {
        let file = image.file_name().unwrap().to_str().unwrap();
        if let Some(hash) = self.case["images"][file].as_str() {
            fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/ui-application-lifecycle/blobs")
                    .join(hash),
                image,
            )?;
        }
        let state = image.with_extension("json");
        if let Some(text) =
            self.case["files"][state.file_name().unwrap().to_str().unwrap()].as_str()
        {
            fs::write(state, text)?;
        }
        if image.file_stem() == Some(OsStr::new("dynamic")) {
            for (name, _) in self.case["files"].as_object().unwrap() {
                if name.starts_with("dynamic.videos/") {
                    let path = self.output.join(name);
                    fs::create_dir_all(path.parent().unwrap())?;
                    fs::write(path, b"video")?;
                }
            }
        }
        Ok(())
    }
    fn command(&mut self, spec: &CommandSpec, kind: &str, probe: bool) {
        let argv = spec.effective_argv();
        let mut argv = argv
            .iter()
            .map(|v| v.to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        if argv[0] == self.reference.to_str().unwrap() {
            argv.splice(
                0..1,
                [
                    "@PYTHON@".into(),
                    format!(
                        "{}/rust/tools/ui_qa/application_source.py",
                        self.root.display()
                    ),
                ],
            );
        }
        let env = spec.effective_env();
        let mut selected = BTreeMap::new();
        for key in [
            "DISPLAY",
            "PARAMS_ROOT",
            "OPENPILOT_PREFIX",
            "PYTHONPATH",
            "PROFILE_RENDER",
            "PROFILE_STARTUP",
            "OFFSCREEN",
            "RECORD",
            "RECORD_OUTPUT",
        ] {
            if let Some(value) = env.get(OsStr::new(key)) {
                selected.insert(key, value.to_str().unwrap());
            }
        }
        self.calls.push(if kind=="popen" {json!({"kind":kind,"argv":argv,"env":selected,"merge":true})}
        else {json!({"kind":kind,"argv":argv,"env":selected,"timeout":if probe {Value::Null}else{json!(45)},"check":true,"capture_output":probe,"text":probe,"merge":!probe})});
    }
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.command(spec, "run", true);
        assert!(
            spec.inherit_stdin
                && spec.stdout_limit == usize::MAX
                && spec.stderr_limit == usize::MAX
        );
        assert_eq!(spec.timeout, Duration::MAX);
        let dynamic = spec
            .effective_argv()
            .last()
            .unwrap()
            .to_string_lossy()
            .contains("dynamic.videos");
        let name = if dynamic {
            "dynamic-ffprobe.json"
        } else {
            "record-ffprobe.json"
        };
        Ok(Captured {
            code: 0,
            stdout: self.case["files"][name]
                .as_str()
                .unwrap()
                .as_bytes()
                .to_vec(),
            stderr: vec![],
        })
    }
    fn run_to_file(&mut self, spec: &CommandSpec, log: &Path) -> Result<Captured> {
        self.command(spec, "run", false);
        assert_eq!(spec.timeout, Duration::from_secs(45));
        assert!(spec.inherit_stdin);
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        fs::write(log, b"")?;
        let source = spec.effective_argv()[0] == self.reference;
        if self.name()
            == if source {
                "source-timeout"
            } else {
                "native-timeout"
            }
        {
            return Err("original controlled timeout".into());
        }
        if self.name()
            == if source {
                "source-error"
            } else {
                "native-error"
            }
        {
            return Ok(Captured {
                code: 7,
                stdout: vec![],
                stderr: vec![],
            });
        }
        self.author(Path::new(spec.effective_argv().last().unwrap()))?;
        if !source {
            let level = if self.name() == "log-level-mismatch" {
                21
            } else {
                20
            };
            let message = if self.name() == "log-no-raylib" {
                "other"
            } else {
                "raylib: message"
            };
            let pathname = if self.name() == "log-path-mismatch" {
                "wrong.rs"
            } else {
                "/source/logging.rs"
            };
            let payload = format!(
                "{{\"levelnum\": 20, \"msg\": \"{message}\", \"pathname\": \"{pathname}\"}}"
            );
            let mut packet = vec![level];
            packet.extend(payload.as_bytes());
            self.packets.push_back(packet);
        }
        Ok(Captured {
            code: 0,
            stdout: vec![],
            stderr: vec![],
        })
    }
}
impl Backend for Peer {
    fn bind_logs(&mut self, address: &str) -> Result<()> {
        self.xops.push(json!(["socket", 7]));
        self.xops.push(json!(["bind", address]));
        Ok(())
    }
    fn log_packet(&mut self) -> Result<Option<Vec<u8>>> {
        self.xops.push(json!(["poll", 100]));
        Ok(self.packets.pop_front())
    }
    fn close_logs(&mut self) -> Result<()> {
        self.xops.push(json!(["receiver-close"]));
        self.xops.push(json!(["transport-term"]));
        Ok(())
    }
    fn open_display(&mut self, name: &OsStr) -> Result<()> {
        self.xops
            .push(json!(["display-open", name.to_str().unwrap()]));
        Ok(())
    }
    fn window(&mut self, _: &str) -> Result<Option<u64>> {
        Ok((self.name() != "live-no-window").then_some(1))
    }
    fn focus(&mut self, _: u64) -> Result<()> {
        self.xops.push(json!(["focus", 2, 0]));
        Ok(())
    }
    fn sync(&mut self) -> Result<()> {
        self.xops.push(json!(["sync"]));
        Ok(())
    }
    fn key(&mut self, ch: char, down: bool) -> Result<()> {
        self.xops
            .push(json!(["input", if down { 2 } else { 3 }, [ch as u32], {}]));
        Ok(())
    }
    fn motion(&mut self, _: u64, x: i32, y: i32) -> Result<()> {
        self.xops.push(json!(["translate", x, y]));
        self.xops.push(json!(["input",6,[],{"x":x+30,"y":y+20}]));
        Ok(())
    }
    fn button(&mut self, down: bool) -> Result<()> {
        self.xops
            .push(json!(["input", if down { 4 } else { 5 }, [1], {}]));
        Ok(())
    }
    fn close_display(&mut self) -> Result<()> {
        self.xops.push(json!(["display-close"]));
        Ok(())
    }
    fn protocol_errors(&mut self) -> Result<bool> {
        Ok(self.name() == "live-protocol-error")
    }
    fn start_live(&mut self, spec: &CommandSpec, log: &Path) -> Result<u32> {
        self.command(spec, "popen", false);
        fs::write(log, b"")?;
        self.author(Path::new(spec.effective_argv().last().unwrap()))?;
        self.code = if self.name() == "live-early-exit" {
            Some(9)
        } else {
            None
        };
        Ok(424242)
    }
    fn poll_live(&mut self) -> Result<Option<i32>> {
        Ok(self.code)
    }
    fn wait_live(&mut self, timeout: Option<Duration>) -> Result<Option<i32>> {
        self.xops
            .push(json!(["wait", timeout.map(|t| t.as_secs_f64())]));
        if self.name() == "live-never-close" && self.code.is_none() {
            return Ok(None);
        }
        if self.code.is_none() {
            self.code = Some(0);
        }
        Ok(self.code)
    }
    fn kill_live(&mut self) -> Result<()> {
        self.xops.push(json!(["kill"]));
        self.code = Some(-9);
        Ok(())
    }
    fn identity(&mut self, _: u32) -> Result<(PathBuf, String)> {
        Ok((
            if self.name() == "live-wrong-executable" {
                self.output.join("wrong-exe")
            } else {
                self.binary.clone()
            },
            if self.name() == "live-python-mapped" {
                "libpython3.12.so"
            } else {
                "/lib/libc.so"
            }
            .into(),
        ))
    }
    fn now(&mut self) -> f64 {
        self.clock += 0.11;
        self.clock
    }
    fn sleep(&mut self, seconds: f64) {
        self.xops.push(json!(["sleep", seconds]));
    }
}
fn normalize(v: &Value, peer: &Peer) -> Value {
    match v {
        Value::String(s) => json!(s
            .replace(peer.output.to_str().unwrap(), "@OUTPUT@")
            .replace(peer.binary.to_str().unwrap(), "@BINARY@")
            .replace(peer.root.to_str().unwrap(), "@ROOT@")
            .replace("ui-framework-125-777", "ui-framework-125-@PID@")),
        Value::Array(a) => Value::Array(a.iter().map(|v| normalize(v, peer)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), normalize(v, peer)))
                .collect(),
        ),
        v => v.clone(),
    }
}
fn collect(
    path: &Path,
    root: &Path,
    files: &mut BTreeMap<String, Value>,
    images: &mut BTreeMap<String, String>,
) {
    use sha2::{Digest, Sha256};
    for p in fs::read_dir(path).unwrap().map(|p| p.unwrap().path()) {
        if p.is_dir() {
            collect(&p, root, files, images);
            continue;
        }
        let name = p.strip_prefix(root).unwrap().to_str().unwrap().to_owned();
        let bytes = fs::read(&p).unwrap();
        if p.extension() == Some(OsStr::new("png")) {
            images.insert(name, format!("{:x}", Sha256::digest(bytes)));
        } else {
            files.insert(
                name,
                json!(if p.extension() == Some(OsStr::new("mp4")) {
                    "@VIDEO@".into()
                } else {
                    String::from_utf8(bytes).unwrap()
                }),
            );
        }
    }
}
#[test]
fn all_thirty_two_original_checker_contracts_preserve_calls_and_partial_artifacts() {
    let cases: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/checker-original.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path();
        let output = base.join("output");
        let binary = base.join("native-application");
        fs::write(&binary, b"native peer\n").unwrap();
        let options = Options {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            binary: binary.clone(),
            reference: base.join("reference"),
            output: output.clone(),
            display: ":owned".into(),
            pid: 777,
        };
        let mut peer = Peer {
            case: case.clone(),
            output: output.clone(),
            binary,
            root: options.root.clone(),
            reference: options.reference.clone(),
            calls: vec![],
            xops: vec![],
            packets: VecDeque::new(),
            code: None,
            clock: 0.,
        };
        let mut printed = vec![];
        let result = check(&options, &mut peer, &mut printed);
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{}: {result:?}",
            case["name"]
        );
        assert_eq!(
            normalize(&json!(peer.calls), &peer),
            case["calls"],
            "{} child calls",
            case["name"]
        );
        assert_eq!(
            normalize(&json!(peer.xops), &peer),
            case["xops"],
            "{} XTest/log/lifetime calls",
            case["name"]
        );
        let mut files = BTreeMap::new();
        let mut images = BTreeMap::new();
        collect(&output, &output, &mut files, &mut images);
        assert_eq!(
            normalize(&json!(files), &peer),
            case["files"],
            "{} partial files",
            case["name"]
        );
        assert_eq!(
            json!(images),
            case["images"],
            "{} image files",
            case["name"]
        );
        assert_eq!(
            normalize(&json!(String::from_utf8(printed).unwrap()), &peer),
            case["printed"],
            "{} stdout",
            case["name"]
        );
    }
}
