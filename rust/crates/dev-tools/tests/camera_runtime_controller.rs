//! Original continuous runtime controller protocol; no device or Python execution.
#![cfg(feature = "camera-sensor-native")]
use flate2::read::GzDecoder;
use openpilot_dev_tools::{
    camera_runtime_check::{control, Options, Runtime},
    camera_state_check::{NativeSchemaDecoder, SchemaDecoder},
    qa_process::CommandSpec,
    seal::Result,
};
use openpilot_log_format::JsonValue;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

fn fixture_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/camera-runtime-check")
}
struct ControllerPeer<'a> {
    expected: &'a Value,
    root: &'a Path,
    operations: Vec<Value>,
    clock: f64,
    spawned: bool,
    killed: bool,
    signalled: bool,
    returncode: Option<i32>,
    connected: [bool; 3],
    messages: Vec<Vec<Value>>,
    images: Vec<Vec<Value>>,
    message_at: [usize; 3],
    image_at: [usize; 3],
    log_sent: bool,
}
impl ControllerPeer<'_> {
    fn mode(&self) -> &str {
        self.expected["mode"].as_str().unwrap()
    }
    fn case(&self) -> &str {
        self.expected["case"].as_str().unwrap()
    }
    fn log(&mut self, value: Value) {
        let value = serde_json::from_str(
            &serde_json::to_string(&value)
                .unwrap()
                .replace(self.root.to_str().unwrap(), "$ROOT"),
        )
        .unwrap();
        let next = &self.expected["operations"][self.operations.len()];
        assert_eq!(&value, next, "{}/{} operation", self.case(), self.mode());
        self.operations.push(value);
    }
    fn ready(&self) -> bool {
        self.root.join("out/go").exists()
            && (self.case() != "publication-order" || self.root.join("out/register-go").exists())
    }
    fn enabled(&self, port: usize) -> bool {
        port != 1 || !["disabled-road", "missing-road"].contains(&self.case())
    }
}
impl Runtime for ControllerPeer<'_> {
    fn pid(&self) -> u32 {
        4242
    }
    fn inherited_environment(&self) -> Vec<(OsString, OsString)> {
        [
            ("PATH", "/owned/path"),
            ("KEEP", "retained"),
            ("CK_AMBIENT", "removed"),
            ("SPECTRA_AMBIENT", "removed"),
            ("DISABLE_AMBIENT", "removed"),
            ("DEBUG_FRAMES", "removed"),
            ("LOG_RAW_FRAMES", "removed"),
            ("CTRL_EXP_FROM_PARAMS", "removed"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
    }
    fn create_namespace(&mut self, path: &Path) -> Result<()> {
        self.log(serde_json::json!(["namespace-create", path]));
        Ok(())
    }
    fn subscribe(&mut self, service: &str, conflate: bool, segment_size: usize) -> Result<()> {
        self.log(serde_json::json!(["subscribe",service,{"conflate":conflate,"segment_size":segment_size}]));
        if self.mode() == "subscriber-setup" {
            return Err("controlled subscriber setup".into());
        }
        Ok(())
    }
    fn vision_new(&mut self, name: &str, port: usize, conflate: bool) -> Result<()> {
        self.log(serde_json::json!(["vision-new", name, port, conflate]));
        Ok(())
    }
    fn logger_socket(&mut self) -> Result<()> {
        self.log(serde_json::json!(["logger-socket", 7]));
        Ok(())
    }
    fn logger_linger_zero(&mut self) -> Result<()> {
        self.log(serde_json::json!(["logger-option", 17, 0]));
        Ok(())
    }
    fn logger_bind(&mut self, endpoint: &str) -> Result<()> {
        self.log(serde_json::json!(["logger-bind", endpoint]));
        if self.mode() == "logger-bind" {
            return Err("controlled logger bind".into());
        }
        Ok(())
    }
    fn spawn(&mut self, spec: &CommandSpec, stdout: &Path, stderr: &Path) -> Result<()> {
        assert!(spec.inherit_stdin);
        assert!(spec.input.is_none());
        assert!(!spec.disable_core_dump);
        let command = spec
            .effective_argv()
            .iter()
            .map(|v| v.to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let env = spec
            .effective_env()
            .into_iter()
            .map(|(k, v)| (k.into_string().unwrap(), v.into_string().unwrap()))
            .collect::<BTreeMap<_, _>>();
        self.log(serde_json::json!(["spawn", command, env]));
        // The common controller, not the peer, must open both artifact streams.
        assert!(stdout.is_file() && stderr.is_file());
        if self.mode() == "spawn" {
            return Err("controlled missing child".into());
        }
        self.spawned = true;
        if !["startup-timeout", "startup-exit"].contains(&self.mode()) {
            fs::write(
                self.root.join("out/trace.jsonl"),
                "{\"op\":\"runtime-ready\"}\n",
            )?;
        }
        if self.case() == "publication-order" {
            fs::write(self.root.join("out/register-go.ready"), "ready")?;
        }
        Ok(())
    }
    fn poll(&mut self) -> Result<Option<i32>> {
        self.log(serde_json::json!(["poll"]));
        if self.killed {
            self.returncode = Some(-9);
        } else if self.mode() == "startup-exit" {
            self.returncode = Some(3);
        } else if self.ready()
            && (self.signalled || self.case() == "poll-error")
            && self.mode() != "runtime-timeout"
        {
            self.returncode = Some(if self.mode() == "runtime-exit" { 2 } else { 0 });
        }
        Ok(self.returncode)
    }
    fn signal(&mut self, signal: i32) -> Result<()> {
        self.log(serde_json::json!(["signal", signal]));
        self.signalled = true;
        Ok(())
    }
    fn kill(&mut self) -> Result<()> {
        self.log(serde_json::json!(["kill"]));
        self.killed = true;
        Ok(())
    }
    fn wait(&mut self) -> Result<i32> {
        self.log(serde_json::json!(["wait"]));
        self.returncode = Some(-9);
        Ok(-9)
    }
    fn monotonic(&self) -> Duration {
        Duration::from_secs_f64(self.clock)
    }
    fn sleep(&mut self, duration: Duration) {
        self.log(serde_json::json!(["sleep", duration.as_secs_f64()]));
        self.clock += duration.as_secs_f64() * 1000.0;
    }
    fn receive_message(&mut self, port: usize) -> Result<Option<Vec<u8>>> {
        let index = self.message_at[port];
        if self.mode() == "receive-error" && self.spawned && self.ready() && port == 0 && index == 1
        {
            return Err("controlled receive failure".into());
        }
        if !self.spawned
            || !self.enabled(port)
            || !self.ready()
            || index >= self.messages[port].len().min(5)
        {
            return Ok(None);
        }
        self.message_at[port] += 1;
        if self.mode() == "bad-wire" {
            return Ok(Some(b"x".to_vec()));
        }
        let selected = if self.mode() == "wrong-service" && port == 0 {
            1
        } else {
            port
        };
        let wire = self.messages[selected][index]["wire"].as_str().unwrap();
        Ok(Some(
            (0..wire.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&wire[i..i + 2], 16).unwrap())
                .collect(),
        ))
    }
    fn vision_connected(&self, port: usize) -> bool {
        self.connected[port]
    }
    fn vision_connect(&mut self, port: usize) -> Result<bool> {
        self.log(serde_json::json!(["vision-connect", port, false]));
        self.connected[port] = self.mode() != "vision-timeout";
        Ok(self.connected[port])
    }
    fn receive_image(&mut self, port: usize) -> Result<Option<JsonValue>> {
        let index = self.image_at[port];
        let early = self.case() == "publication-order" && !self.ready() && port == 0 && index == 0;
        if (!self.ready() && !early)
            || (self.mode() == "publication-mismatch" && early)
            || index >= self.images[port].len().min(5)
        {
            return Ok(None);
        }
        self.image_at[port] += 1;
        Ok(Some(JsonValue::parse(
            &self.images[port][index].to_string(),
        )?))
    }
    fn receive_log(&mut self) -> Result<Option<Vec<u8>>> {
        if !self.spawned || self.log_sent {
            return Ok(None);
        }
        self.log_sent = true;
        if self.mode() == "bad-log-json" {
            return Ok(Some(b"\x14broken".to_vec()));
        }
        let mut bytes = vec![if self.mode() == "bad-log-level" {
            30
        } else {
            20
        }];
        bytes.extend(br#"{"levelnum":20,"msg":"controlled logging"}"#);
        Ok(Some(bytes))
    }
    fn logger_close(&mut self) -> Result<()> {
        self.log(serde_json::json!(["logger-close"]));
        Ok(())
    }
    fn context_term(&mut self) -> Result<()> {
        self.log(serde_json::json!(["context-term"]));
        Ok(())
    }
    fn remove_namespace(&mut self, path: &Path) -> Result<()> {
        self.log(serde_json::json!(["namespace-remove", path]));
        Ok(())
    }
    fn unlink_endpoint(&mut self, path: &Path) -> Result<()> {
        self.log(serde_json::json!(["endpoint-unlink", path]));
        Ok(())
    }
}
fn controller_cases(early_setup: bool) {
    controller_cases_file(
        early_setup,
        "controller-original.json",
        if early_setup { 3 } else { 21 },
    );
}
fn controller_cases_file(early_setup: bool, fixture: &str, expected_count: usize) {
    let captures: Value =
        serde_json::from_slice(&fs::read(fixture_directory().join(fixture)).unwrap()).unwrap();
    let program = std::env::var_os("OPENPILOT_CAPNP").unwrap_or_else(|| "capnp".into());
    let mut decoder = NativeSchemaDecoder::new(program.into());
    decoder
        .load(&fixture_directory().join("schema/log.capnp"))
        .unwrap();
    let mut count = 0;
    for captured in captures["cases"].as_array().unwrap() {
        // These pre-try setup failures are covered separately after exact original
        // finalization: native owned cleanup improves the original leaked namespace.
        let setup_failure = ["subscriber-setup", "logger-bind", "qemu-missing-sysroot"]
            .contains(&captured["mode"].as_str().unwrap());
        if setup_failure != early_setup {
            continue;
        }
        let mut expected = captured.clone();
        if setup_failure {
            // Explicit owned-cleanup improvement over the captured pre-finally
            // leak; original artifact and error expectations stay unchanged.
            let operations = expected["operations"].as_array_mut().unwrap();
            if captured["mode"] == "logger-bind" {
                operations.push(serde_json::json!(["logger-close"]));
                operations.push(serde_json::json!(["context-term"]));
            }
            operations.push(serde_json::json!([
                "namespace-remove",
                "/dev/shm/msgq_camera_runtime_4242"
            ]));
        }
        let case = &expected;
        count += 1;
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let output = root.join("out");
        if case["mode"] == "output-exists" {
            fs::create_dir(&output).unwrap();
        }
        let baseline: Value = serde_json::from_reader(GzDecoder::new(
            fs::File::open(fixture_directory().join(format!(
                "{}-original.json.gz",
                case["case"].as_str().unwrap()
            )))
            .unwrap(),
        ))
        .unwrap();
        let expected_messages: Value =
            serde_json::from_str(baseline["files"]["messages.json"].as_str().unwrap()).unwrap();
        let expected_images: Value =
            serde_json::from_str(baseline["files"]["vision.json"].as_str().unwrap()).unwrap();
        let mut peer = ControllerPeer {
            expected: case,
            root,
            operations: vec![],
            clock: 0.0,
            spawned: false,
            killed: false,
            signalled: false,
            returncode: None,
            connected: [false; 3],
            messages: expected_messages
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_array().unwrap().clone())
                .collect(),
            images: expected_images
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_array().unwrap().clone())
                .collect(),
            message_at: [0; 3],
            image_at: [0; 3],
            log_sent: false,
        };
        let mode = case["mode"].as_str().unwrap();
        let options = Options {
            binary: root.join("binary"),
            fixture: root.join("fixture"),
            oracle: root.join("oracle"),
            output: output.clone(),
            case: case["case"].as_str().unwrap().into(),
            qemu: mode.starts_with("qemu").then(|| root.join("qemu")),
            sysroot: (mode == "qemu").then(|| root.join("sysroot")),
        };
        let result = control(&options, &mut peer, &mut decoder);
        assert_eq!(
            result.is_ok(),
            case["success"].as_bool().unwrap(),
            "{}/{}: {:?}",
            options.case,
            mode,
            result.err().map(|e| e.to_string())
        );
        let expected_operations = case["operations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v[0] != "verify")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            peer.operations, expected_operations,
            "{}/{mode} operations",
            options.case
        );
        let mut files = serde_json::Map::new();
        if output.exists() {
            let mut paths = vec![output];
            while let Some(path) = paths.pop() {
                for entry in fs::read_dir(path).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        paths.push(path);
                    } else {
                        files.insert(
                            path.strip_prefix(root.join("out"))
                                .unwrap()
                                .to_str()
                                .unwrap()
                                .into(),
                            fs::read_to_string(&path)
                                .unwrap()
                                .replace(root.to_str().unwrap(), "$ROOT")
                                .into(),
                        );
                    }
                }
            }
        }
        assert_eq!(
            Value::Object(files),
            case["files"],
            "{}/{mode} exact artifacts",
            options.case
        );
    }
    assert_eq!(count, expected_count);
}
#[test]
fn original_controller_cases_signals_timeouts_and_partial_receipts_are_preserved() {
    controller_cases(false);
}
#[test]
fn early_collector_failure_preserves_original_artifacts_and_cleans_only_owned_resources() {
    controller_cases(true);
}

#[test]
fn original_receive_error_preserves_the_successfully_decoded_prefix() {
    controller_cases_file(false, "controller-receive-original.json", 1);
}
