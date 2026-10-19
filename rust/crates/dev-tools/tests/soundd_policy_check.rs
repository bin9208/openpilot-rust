//! Dynamic comparison boundaries using independent source-captured child replies.
#![cfg(feature = "soundd-policy-native")]
#[path = "support/soundd_policy_json.rs"]
mod soundd_policy_json;

use flate2::read::GzDecoder;
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    soundd_policy_check::{run, Options},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
}
fn corpus() -> Value {
    let bytes = include_bytes!("fixtures/soundd-policy/original.json.gz");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "82ef895663d96cfde1f33da0e22c4785a007c05a2dcab43db0b9e76a2c4c54b1"
    );
    let mut raw = Vec::new();
    GzDecoder::new(bytes.as_slice())
        .read_to_end(&mut raw)
        .unwrap();
    soundd_policy_json::parse(&raw)
}
fn options(work: &Path, original: &Value) -> Options {
    let folder = work.join("binding");
    fs::create_dir(&folder).unwrap();
    let binding = folder.join("params_pyx.so");
    // The new controller reads this artifact only for provenance admission;
    // reference expectations are computed independently, never loaded from it.
    fs::write(&binding, b"owned opaque provenance-only fixture").unwrap();
    let mut provenance = original["provenance"]["params_binding"].clone();
    provenance["module_sha256"] =
        json!(format!("{:x}", Sha256::digest(fs::read(&binding).unwrap())));
    fs::write(
        folder.join("provenance.json"),
        serde_json::to_vec(&provenance).unwrap(),
    )
    .unwrap();
    Options {
        binary: PathBuf::from("/owned/sound_trace"),
        binding,
        output: work.join("output"),
    }
}
struct Peer<'a> {
    original: &'a Value,
    calls: usize,
    corruption: Option<&'static str>,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert_eq!(
            spec.effective_argv(),
            vec![std::ffi::OsString::from("/owned/sound_trace")]
        );
        assert_eq!(spec.timeout, Duration::from_secs(30));
        assert!(!spec.inherit_output && !spec.inherit_stdin && !spec.merge_output);
        let mut input: Value = soundd_policy_json::parse(spec.input.as_ref().unwrap());
        let row = &self.original["cases"][self.calls];
        assert!(Path::new(input["assets"].as_str().unwrap()).is_dir());
        input["assets"] = row["config"]["assets"].clone();
        if input.get("basedir").is_some() {
            input["basedir"] = row["config"]["basedir"].clone();
        }
        assert_eq!(input, row["config"]);
        self.calls += 1;
        let mut reply = row["intended"].as_array().unwrap().clone();
        for item in &mut reply {
            item["unsupported"] = Value::Null;
        }
        match self.corruption {
            Some("sample") => reply[0]["samples"][0] = json!(0.25),
            Some("alert") => reply[0]["state"]["alert"] = json!(1),
            Some("scalar") => reply[0]["state"]["filter"] = json!(24.01),
            Some("count") => {
                reply.pop();
            }
            Some("json") => {
                return Ok(Captured {
                    code: 0,
                    stdout: b"{bad json}\n".to_vec(),
                    stderr: vec![],
                })
            }
            Some("utf8") => {
                return Ok(Captured {
                    code: 0,
                    stdout: vec![255],
                    stderr: vec![],
                })
            }
            Some("exit") => {
                return Ok(Captured {
                    code: 7,
                    stdout: vec![],
                    stderr: b"owned failed child".to_vec(),
                })
            }
            _ => (),
        }
        let mut stdout = b"source-compatible non-JSON diagnostic\n".to_vec();
        for item in reply {
            stdout.extend(serde_json::to_vec(&item)?);
            stdout.push(b'\n');
        }
        Ok(Captured {
            code: 0,
            stdout,
            stderr: vec![],
        })
    }
}

#[test]
fn full_six_scenario_controller_uses_every_original_input_and_assertion() {
    let original = corpus();
    let work = tempfile::tempdir().unwrap();
    let options = options(work.path(), &original);
    let mut peer = Peer {
        original: &original,
        calls: 0,
        corruption: None,
    };
    let summary: Value = run(&options, root(), &mut peer).unwrap();
    assert_eq!(peer.calls, 6);
    assert_eq!(summary["passed"], true);
    assert_eq!(summary["scenarios"], 6);
    assert_eq!(summary["steps_per_scenario"], 141);
    assert_eq!(summary["sample_comparisons"], 6348);
    assert_eq!(summary["original_unchanged_sample_comparisons"], 5235);
    assert_eq!(summary["intentional_clip_boundary_corrected_samples"], 1113);
    assert_eq!(
        summary["intentional_clip_boundary_corrected_frame_rows"],
        27
    );
    assert_eq!(summary["all_nonplayback_state_source_exact"], true);
    assert_eq!(summary["audio_initialized"], false);
    for index in 0..6 {
        for suffix in [
            "input.json",
            "source.json",
            "intended.json",
            "native.json",
            "stderr.log",
        ] {
            assert!(options.output.join(format!("{index}-{suffix}")).is_file());
        }
    }
    let saved: Value =
        serde_json::from_slice(&fs::read(options.output.join("summary.json")).unwrap()).unwrap();
    assert_eq!(saved, summary);
}

#[test]
fn corruption_and_child_failures_never_leave_a_success_receipt() {
    let original = corpus();
    for corruption in ["sample", "alert", "scalar", "count", "json", "utf8", "exit"] {
        let work = tempfile::tempdir().unwrap();
        let options = options(work.path(), &original);
        fs::create_dir(&options.output).unwrap();
        fs::write(options.output.join("summary.json"), b"{\"passed\":true}").unwrap();
        let mut peer = Peer {
            original: &original,
            calls: 0,
            corruption: Some(corruption),
        };
        assert!(run(&options, root(), &mut peer).is_err(), "{corruption}");
        assert_eq!(peer.calls, 1, "{corruption}");
        assert!(
            !options.output.join("summary.json").exists(),
            "{corruption}"
        );
        if matches!(corruption, "json" | "utf8" | "exit") {
            assert!(
                !options.output.join("0-source.json").exists(),
                "{corruption}"
            );
        }
    }
}

#[test]
fn binding_admission_precedes_child_execution_and_clears_stale_success() {
    let original = corpus();
    let work = tempfile::tempdir().unwrap();
    let options = options(work.path(), &original);
    fs::create_dir(&options.output).unwrap();
    fs::write(options.output.join("summary.json"), b"{\"passed\":true}").unwrap();
    fs::write(&options.binding, b"changed artifact").unwrap();
    let mut peer = Peer {
        original: &original,
        calls: 0,
        corruption: None,
    };
    assert!(run(&options, root(), &mut peer).is_err());
    assert_eq!(peer.calls, 0);
    assert!(!options.output.join("summary.json").exists());
}

#[test]
fn actual_daemon_cli_advertises_and_requires_native_fixture_helper_before_effects() {
    use std::process::Command;
    // Sensor/audio CI already provides the retained NumPy/capnp/msgq imports.
    // Local gates explicitly select the existing read-only original venv.
    let python = std::env::var_os("SOUNDD_DAEMON_PYTHON").unwrap_or_else(|| "python".into());
    let caller = root().join("rust/tools/check_soundd_daemon.py");
    let help = Command::new(&python)
        .arg("-P")
        .arg(&caller)
        .arg("--help")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap();
    assert!(help.status.success(), "{:?}", help.stderr);
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("--fixture-assets-binary"));
    assert!(help.contains("--portaudio-library"));
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("must-not-be-created");
    let missing = Command::new(&python)
        .arg("-P")
        .arg(caller)
        .args(["--binary", "/bin/false", "--output"])
        .arg(&output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2), "{:?}", missing.stderr);
    let error = String::from_utf8(missing.stderr).unwrap();
    assert!(error.contains("--fixture-assets-binary"));
    assert!(!error.contains("AttributeError"));
    assert!(!output.exists());
}
