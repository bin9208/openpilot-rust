//! Failure/callback order captured by invoking unchanged GuiApplication methods.
#![cfg(feature = "image-array-native")]
use openpilot_dev_tools::ui_application_lifecycle_recording::{
    close, dynamic_command, global_command, CloseError, ClosePeer, GlobalOptions,
};
use serde_json::{json, Value};
use std::{cell::RefCell, ffi::OsStr, time::Duration};

struct Peer<'a> {
    name: &'a str,
    calls: RefCell<Vec<Value>>,
    waits: usize,
    detached: bool,
    active: bool,
}
impl Peer<'_> {
    fn call(&self, fields: Value) {
        let mut fields = fields.as_array().unwrap().clone();
        fields.push(self.detached.into());
        self.calls.borrow_mut().push(fields.into());
    }
}
impl ClosePeer for Peer<'_> {
    fn detach(&mut self) {
        self.active = !self.detached;
        self.detached = true;
    }
    fn has_thread(&self) -> bool {
        self.active && !matches!(self.name, "empty" | "queue-no-thread")
    }
    fn has_event(&self) -> bool {
        self.active && !matches!(self.name, "empty" | "thread-no-event")
    }
    fn has_queue(&self) -> bool {
        self.active && self.name != "empty"
    }
    fn has_process(&self) -> bool {
        self.active && self.name != "empty"
    }
    fn stdin_open(&self) -> bool {
        self.active && !matches!(self.name, "no-stdin" | "stdin-already-closed")
    }
    fn event_set(&mut self) -> Result<(), CloseError> {
        self.call(json!(["event-set"]));
        if self.name == "event-error" {
            Err(CloseError::Other)
        } else {
            Ok(())
        }
    }
    fn queue_sentinel(&mut self) -> Result<(), CloseError> {
        self.call(json!(["sentinel", null]));
        if self.name == "queue-full" {
            Err(CloseError::Other)
        } else {
            Ok(())
        }
    }
    fn join(&mut self, timeout: Duration) -> Result<(), CloseError> {
        self.call(json!(["thread-join", timeout.as_secs()]));
        if self.name == "thread-join-error" {
            Err(CloseError::Other)
        } else {
            Ok(())
        }
    }
    fn flush(&mut self) -> Result<(), CloseError> {
        self.call(json!(["stdin-flush"]));
        if self.name == "flush-error" {
            Err(CloseError::Other)
        } else {
            Ok(())
        }
    }
    fn close_stdin(&mut self) -> Result<(), CloseError> {
        self.call(json!(["stdin-close"]));
        if self.name == "close-error" {
            Err(CloseError::Other)
        } else {
            Ok(())
        }
    }
    fn wait(&mut self, timeout: Duration) -> Result<(), CloseError> {
        self.waits += 1;
        self.call(json!(["process-wait", timeout.as_secs()]));
        if matches!(self.name, "wait-other-error" | "kill-error") {
            Err(CloseError::Other)
        } else if matches!(
            self.name,
            "wait-timeout" | "terminate-error" | "second-wait-timeout"
        ) && (self.waits == 1 || self.name == "second-wait-timeout")
        {
            Err(CloseError::Timeout)
        } else {
            Ok(())
        }
    }
    fn terminate(&mut self) -> Result<(), CloseError> {
        self.call(json!(["process-terminate"]));
        if self.name == "terminate-error" {
            Err(CloseError::Other)
        } else {
            Ok(())
        }
    }
    fn kill(&mut self) -> Result<(), CloseError> {
        self.call(json!(["process-kill"]));
        if self.name == "kill-error" {
            Err(CloseError::Other)
        } else {
            Ok(())
        }
    }
}
#[test]
fn original_recording_cleanup_detach_callbacks_timeouts_and_dynamic_arguments() {
    let source: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/recording-policy-original.json"
    ))
    .unwrap();
    for case in source["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        if let Some(fps) = name.strip_prefix("dynamic-init-fps-") {
            let command = dynamic_command(
                536,
                240,
                fps.parse().unwrap(),
                OsStr::new("/capture/record.mp4"),
            );
            assert_eq!(
                serde_json::to_value(
                    command
                        .iter()
                        .map(|v| v.to_str().unwrap())
                        .collect::<Vec<_>>()
                )
                .unwrap(),
                case["calls"][1][1],
                "{name}"
            );
            continue;
        }
        let mut peer = Peer {
            name,
            calls: RefCell::new(vec![]),
            waits: 0,
            detached: false,
            active: false,
        };
        close(&mut peer);
        assert!(
            peer.detached,
            "references must be detached before any callback"
        );
        assert_eq!(
            *peer.calls.borrow(),
            *case["calls"].as_array().unwrap(),
            "{name}"
        );
        let first_count = peer.calls.borrow().len();
        close(&mut peer);
        assert_eq!(
            peer.calls.borrow().len() - first_count,
            case["second_call_added"].as_u64().unwrap() as usize,
            "{name}: repeated cleanup"
        );
        assert_eq!(case["all_refs_cleared"], true);
    }
}

#[test]
fn global_recording_arguments_preserve_independent_quality_speed_and_bitrate() {
    let source: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/global-recording-original.json"
    ))
    .unwrap();
    for case in source["cases"].as_array().unwrap() {
        let Some(expected) = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v[0] == "popen")
        else {
            continue;
        };
        let settings = &case["settings"];
        let options = GlobalOptions {
            quality: settings["RECORD_QUALITY"].as_i64().unwrap().into(),
            speed: settings["RECORD_SPEED"].as_i64().unwrap().into(),
            bitrate: settings["RECORD_BITRATE"].as_str().unwrap().into(),
        };
        let args = global_command(536, 240, 20, &options, OsStr::new("/capture/global.mp4"));
        let args =
            serde_json::to_value(args.iter().map(|v| v.to_str().unwrap()).collect::<Vec<_>>())
                .unwrap();
        assert_eq!(args, expected[1], "{}", case["name"]);
    }
}

#[test]
fn unchanged_original_writer_queue_sentinel_stop_partial_write_and_failure_scopes() {
    use openpilot_dev_tools::ui_application_lifecycle_recording::{
        write_frames, WriterError, WriterPeer, WriterRead,
    };
    struct Writer {
        items: Vec<Value>,
        stop: bool,
        error: Value,
        calls: Vec<Value>,
        reads: usize,
    }
    impl WriterPeer for Writer {
        fn read(&mut self, timeout: Duration) -> Result<WriterRead, WriterError> {
            if self.error == "missing-queue" {
                return Err(WriterError::Failure);
            }
            self.calls.push(json!(["get", timeout.as_secs_f64()]));
            let item = &self.items[self.reads];
            self.reads += 1;
            match item {
                Value::Null => Ok(WriterRead::Sentinel),
                Value::String(value) if value == "empty" => Err(WriterError::Empty),
                Value::String(value) if value == "error" => Err(WriterError::Failure),
                Value::String(value) => {
                    let bytes = (0..value.len())
                        .step_by(2)
                        .map(|at| u8::from_str_radix(&value[at..at + 2], 16).unwrap())
                        .collect();
                    Ok(WriterRead::Frame(bytes))
                }
                _ => unreachable!(),
            }
        }
        fn stopped(&mut self) -> Result<bool, WriterError> {
            if self.error == "event" {
                return Err(WriterError::Failure);
            }
            self.calls.push(json!(["is-set"]));
            Ok(self.stop)
        }
        fn write(&mut self, bytes: &[u8]) -> Result<usize, WriterError> {
            if matches!(self.error.as_str(), Some("process" | "stdin")) {
                return Err(WriterError::Failure);
            }
            self.calls.push(json!([
                "write",
                bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
            ]));
            if matches!(self.error.as_str(), Some("broken" | "write")) {
                Err(WriterError::Failure)
            } else if self.error == "partial" {
                Ok(1)
            } else {
                Ok(bytes.len())
            }
        }
    }
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/writer-original.json"
    ))
    .unwrap();
    for case in original["cases"].as_array().unwrap() {
        let mut peer = Writer {
            items: case["items"].as_array().unwrap().clone(),
            stop: case["stop"].as_bool().unwrap(),
            error: if case["name"] == "missing-queue" {
                "missing-queue".into()
            } else {
                case["error"].clone()
            },
            calls: vec![],
            reads: 0,
        };
        let result = write_frames(&mut peer);
        assert_eq!(
            result.is_err(),
            !case["exception"].is_null(),
            "{}",
            case["name"]
        );
        assert_eq!(
            peer.calls,
            *case["calls"].as_array().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            peer.items[peer.reads..],
            *case["remaining"].as_array().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn unchanged_original_dynamic_recording_transition_and_exception_order() {
    use openpilot_dev_tools::ui_application_lifecycle_recording::{Dynamic, DynamicPeer};
    struct Peer {
        name: String,
        error: Value,
        texture: bool,
        calls: Vec<Value>,
    }
    impl DynamicPeer for Peer {
        fn ensure_texture(&mut self) -> Result<bool, String> {
            if !self.texture {
                self.calls.push(json!(["target", 536, 240]));
                if self.error == "target" {
                    return Err("target".into());
                }
                self.texture = self.error != "invalid";
                if !self.texture {
                    // Original ensure dereferences target.texture for filtering
                    // before start_recording can test target truthiness.
                    return Err("missing texture filter target".into());
                }
                self.calls.push(json!(["filter", "new-texture", 1]));
            }
            Ok(self.texture)
        }
        fn new_path(&mut self) -> Result<std::path::PathBuf, String> {
            self.calls.push(json!(["new-path"]));
            if self.error == "path" {
                Err("path".into())
            } else {
                Ok("/capture/dynamic.mp4".into())
            }
        }
        fn init(&mut self, path: &std::path::Path, enabled: bool) -> Result<(), String> {
            self.calls
                .push(json!(["init", path.to_str().unwrap(), enabled]));
            if self.error == "spawn" {
                Err("ffmpeg".into())
            } else {
                Ok(())
            }
        }
        fn clock(&mut self, enabled: bool) -> Result<f64, String> {
            self.calls.push(json!(["clock", enabled]));
            if self.error == "clock" {
                Err("clock".into())
            } else {
                Ok(11.0)
            }
        }
        fn close(&mut self, enabled: bool) -> Result<(), String> {
            self.calls.push(json!(["close", enabled]));
            if self.error == "close" {
                Err("close".into())
            } else {
                Ok(())
            }
        }
    }
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/dynamic-recording-original.json"
    ))
    .unwrap();
    for case in original["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut state = Dynamic {
            enabled: case["initial_enabled"].as_bool().unwrap(),
            started: 7.0,
        };
        let mut peer = Peer {
            name: name.into(),
            error: case["error"].clone(),
            texture: !case["initial_texture"].is_null(),
            calls: vec![],
        };
        let result = if peer.name.starts_with("stop") {
            state.stop(&mut peer)
        } else {
            state.start(&mut peer)
        };
        assert_eq!(result.is_err(), !case["exception"].is_null(), "{name}");
        assert_eq!(peer.calls, *case["calls"].as_array().unwrap(), "{name}");
        assert_eq!(state.enabled, case["enabled"].as_bool().unwrap(), "{name}");
        assert_eq!(
            state.started,
            case["start_time"].as_f64().unwrap(),
            "{name}"
        );
        assert_eq!(
            peer.texture,
            case["has_texture"].as_bool().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn actual_encoder_owns_native_child_fifo_stdin_eof_reap_and_repeated_close() {
    use openpilot_dev_tools::ui_application_lifecycle_recording::{Encoder, SubmitError};
    use std::{
        fs,
        process::{Command, Stdio},
        thread,
        time::Instant,
    };
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("native-output.bin");
    let mut command = Command::new("/bin/cat");
    command.stdout(Stdio::from(fs::File::create(&output).unwrap()));
    let mut encoder = Encoder::spawn(&mut command, 8).unwrap();
    let pid = encoder.pid().unwrap();
    let mut expected = Vec::new();
    for i in 0..3u8 {
        let frame = vec![i; 2048];
        expected.extend_from_slice(&frame);
        encoder.submit(frame).unwrap();
    }
    let start = Instant::now();
    while encoder.written_frames() != 3 {
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(2));
    }
    encoder.close();
    assert_eq!(fs::read(&output).unwrap(), expected);
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert_eq!(encoder.submit(vec![1]), Err(SubmitError::Closed));
    assert_eq!(encoder.pid(), None);
    encoder.close();
}

#[test]
fn actual_encoder_spawn_failure_and_broken_pipe_keep_owned_cleanup() {
    use openpilot_dev_tools::ui_application_lifecycle_recording::Encoder;
    use std::{process::Command, thread};
    assert!(Encoder::spawn(&mut Command::new("/nonexistent/application-encoder"), 8).is_err());
    let mut encoder = Encoder::spawn(&mut Command::new("/bin/true"), 8).unwrap();
    let pid = encoder.pid().unwrap();
    thread::sleep(Duration::from_millis(20));
    let _ = encoder.submit(vec![0; 131072]);
    encoder.close();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    encoder.close();
}

#[test]
fn actual_exited_encoder_keeps_original_queue_capacity_after_writer_failure() {
    use openpilot_dev_tools::ui_application_lifecycle_recording::{Encoder, SubmitError};
    use std::{process::Command, thread, time::Instant};
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/writer-queue-original.json"
    ))
    .unwrap();
    for case in original["cases"].as_array().unwrap() {
        let capacity = case["capacity"].as_u64().unwrap() as usize;
        let mut encoder = Encoder::spawn(&mut Command::new("/bin/true"), capacity).unwrap();
        let pid = encoder.pid().unwrap();
        // waitid(WNOWAIT) observes actual child exit without reaping the owner's
        // child, so the large write deterministically encounters a closed pipe.
        let child = rustix::process::Pid::from_raw(i32::try_from(pid).unwrap()).unwrap();
        let start = Instant::now();
        loop {
            let exited = rustix::process::waitid(
                rustix::process::WaitId::Pid(child),
                rustix::process::WaitIdOptions::EXITED
                    | rustix::process::WaitIdOptions::NOHANG
                    | rustix::process::WaitIdOptions::NOWAIT,
            )
            .unwrap();
            if exited.is_some() {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(2));
        }
        encoder.submit(vec![0; 131072]).unwrap();
        let start = Instant::now();
        while !encoder.writer_finished() {
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(2));
        }
        for _ in 0..case["admitted_after_failure"].as_u64().unwrap() {
            assert_eq!(encoder.submit(vec![1]), Ok(()), "queue {capacity}");
        }
        assert_eq!(encoder.submit(vec![2]), Err(SubmitError::Full));
        encoder.close();
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        assert_eq!(encoder.submit(vec![3]), Err(SubmitError::Closed));
    }
}

#[test]
fn actual_short_reading_encoder_rejects_partial_frame_and_keeps_queue_capacity() {
    use openpilot_dev_tools::ui_application_lifecycle_recording::{Encoder, SubmitError};
    use std::{
        process::{Command, Stdio},
        thread,
        time::Instant,
    };

    // dd consumes one 4096-byte block and exits while a much larger frame is
    // being written. A single pipe write can report success for only a prefix.
    let mut command = Command::new("/bin/dd");
    command
        .args(["bs=4096", "count=1", "status=none"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut encoder = Encoder::spawn(&mut command, 8).unwrap();
    let pid = encoder.pid().unwrap();
    encoder.submit(vec![0; 4 * 1024 * 1024]).unwrap();
    let start = Instant::now();
    while !encoder.writer_finished() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "short-reading child left a partial frame accepted as complete"
        );
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(encoder.written_frames(), 0, "partial frame was accepted");
    for _ in 0..8 {
        assert_eq!(encoder.submit(vec![1]), Ok(()));
    }
    assert_eq!(encoder.submit(vec![2]), Err(SubmitError::Full));
    encoder.close();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}
