#![cfg(all(target_os = "linux", feature = "controlsd-fixture-native"))]
use openpilot_cereal::car_capnp::car_params;
// Compile the exact private ownership helper for contract injection, not a copy.
pub use openpilot_dev_tools::seal;
#[allow(dead_code)]
#[path = "../src/control_failures_check/evidence.rs"]
mod evidence_contract;
use openpilot_dev_tools::control_failures_check::{fixtures, run, Options};
use openpilot_dev_tools::qa_process::{Captured, CommandSpec, Runner};
use openpilot_dev_tools::seal::Result;
use serde_json::{json, Value};
use std::{fs, path::Path, sync::atomic::AtomicBool};

fn cp(case: &Value) -> capnp::message::Reader<capnp::serialize::OwnedSegments> {
    let bytes: Vec<u8> = serde_json::from_value(case["params"]["CarParams"].clone()).unwrap();
    capnp::serialize::read_message(&bytes[..], capnp::message::ReaderOptions::new()).unwrap()
}

#[test]
fn all_source_mutations_and_frame_limits_are_preserved() {
    let cases = fixtures::cases().unwrap();
    assert_eq!(cases.len(), 27);
    assert_eq!(
        cases
            .iter()
            .map(|c| c["frames"].as_array().unwrap().len())
            .sum::<usize>(),
        70
    );
    assert_eq!(
        cases
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        fixtures::names()
    );
    assert_eq!(
        cases[0]["params"]["StoppingAccel"],
        json!(b"not a number".to_vec())
    );
    assert_eq!(cases[6]["params"]["FingerPrints"], json!([]));
    assert_eq!(cases[9]["params"]["FingerPrints"], json!([255]));
    assert_eq!(cases[21]["name"], "unknown-fingerprint");
    assert_eq!(
        cp(&cases[21])
            .get_root::<car_params::Reader<'_>>()
            .unwrap()
            .get_car_fingerprint()
            .unwrap()
            .to_str()
            .unwrap(),
        "MISSING"
    );
    assert_eq!(
        cp(&cases[22])
            .get_root::<car_params::Reader<'_>>()
            .unwrap()
            .get_wheelbase(),
        0.
    );
    let psa = cp(&cases[23]);
    let psa = psa.get_root::<car_params::Reader<'_>>().unwrap();
    assert_eq!(psa.get_brand().unwrap().to_str().unwrap(), "psa");
    assert!(matches!(
        psa.get_lateral_tuning().which().unwrap(),
        car_params::lateral_tuning::Pid(_)
    ));
    let torque_message = cp(&cases[24]);
    let torque_cp = torque_message.get_root::<car_params::Reader<'_>>().unwrap();
    let car_params::lateral_tuning::Torque(torque) =
        torque_cp.get_lateral_tuning().which().unwrap()
    else {
        panic!("expected unchanged torque union");
    };
    assert_eq!(torque.unwrap().get_lat_accel_factor(), 0.);
    for frame in cases[24]["frames"].as_array().unwrap() {
        let packet: Vec<u8> = serde_json::from_value(frame["messages"][3].clone()).unwrap();
        let projection =
            openpilot_dev_tools::controlsd_fixture_generate::project_event(&packet).unwrap();
        assert_eq!(
            projection
                .get("liveTorqueParameters")
                .unwrap()
                .get("useParams")
                .unwrap()
                .to_json()
                .unwrap(),
            "false"
        );
    }
    assert_eq!(cases[25]["simulation"], true);
    assert_eq!(
        cp(&cases[25])
            .get_root::<car_params::Reader<'_>>()
            .unwrap()
            .get_steer_control_type()
            .unwrap(),
        car_params::SteerControlType::Angle
    );
    assert_eq!(cases[26]["params"]["CarParams"], json!([0, 1, 2]));
    assert_eq!(cases[26]["frames"], json!([]));
}

#[test]
fn exact_projection_detects_mutation_instead_of_accepting_only_a_roster() {
    let mut case = fixtures::cases().unwrap().remove(0);
    let before = fixtures::projection(&case).unwrap();
    case["params"]["StoppingAccel"] = json!([48]);
    assert_ne!(before, fixtures::projection(&case).unwrap());
}

// This peer tests orchestration contracts only; it is never source-oracle evidence.
struct Peer {
    calls: usize,
    reject: i32,
    create_output: bool,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert!(spec.timeout.as_secs() <= 30);
        assert_eq!(spec.stdout_limit, 1024 * 1024);
        let argv = spec.effective_argv();
        assert_eq!(argv.len(), 2);
        assert!(Path::new(spec.env.get("CONTROLS_NUMERICS").unwrap()).is_absolute());
        let request: Value = serde_json::from_slice(spec.input.as_ref().unwrap())?;
        let health = self.calls == 0;
        self.calls += 1;
        if health || self.create_output {
            let count = request["cases"][0]["frames"].as_array().unwrap().len();
            fs::write(
                &argv[1],
                serde_json::to_vec(
                    &json!([{"name":request["cases"][0]["name"],"rows":vec![json!({});count]}]),
                )?,
            )?;
        }
        Ok(Captured {
            code: if health { 0 } else { self.reject },
            stdout: vec![],
            stderr: b"controlled rejection".to_vec(),
        })
    }
}
fn setup(temp: &Path) -> Options {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let trace = temp.join("trace");
    fs::write(&trace, b"unit orchestration peer").unwrap();
    let numerics = temp.join("numerics");
    fs::create_dir(&numerics).unwrap();
    fs::write(numerics.join("manifest.json"), b"unit peer data").unwrap();
    let cases = fixtures::cases().unwrap();
    let reference = json!({"format":1,"sources":fixtures::source_hashes(&root).unwrap(),"health":{"case":fixtures::health().unwrap(),"projection":fixtures::projection(&fixtures::health().unwrap()).unwrap(),"source_frames":5},"cases":cases.iter().map(|c|json!({"case":c,"projection":fixtures::projection(c).unwrap(),"source":{"exception":"UnitPeerOnly","message":"not original evidence"}})).collect::<Vec<_>>()});
    let reference_path = temp.join("original.json");
    fs::write(&reference_path, serde_json::to_vec(&reference).unwrap()).unwrap();
    let reference_sha256 = openpilot_dev_tools::seal::digest(&reference_path).unwrap();
    Options {
        root,
        trace,
        numerics,
        evidence: temp.join("evidence"),
        reference: reference_path,
        reference_sha256,
    }
}
#[test]
fn nonzero_plus_any_output_is_rejected_and_no_success_receipt_is_left() {
    for (code, output) in [(0, false), (1, true)] {
        let temp = tempfile::tempdir().unwrap();
        let options = setup(temp.path());
        let mut peer = Peer {
            calls: 0,
            reject: code,
            create_output: output,
        };
        assert!(run(&options, &mut peer, &AtomicBool::new(false)).is_err());
        assert!(!options.evidence.join("results.json").exists());
        assert!(options
            .evidence
            .join("malformed-StoppingAccel/input.json")
            .exists());
    }
}
#[test]
fn occupied_evidence_and_cancelled_controller_launch_zero_children() {
    use std::os::unix::fs::symlink;
    for kind in ["file", "directory", "symlink", "dangling", "cancel"] {
        let temp = tempfile::tempdir().unwrap();
        let options = setup(temp.path());
        let foreign = temp.path().join("foreign");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("sentinel"), b"preserve").unwrap();
        match kind {
            "file" => fs::write(&options.evidence, b"occupied").unwrap(),
            "directory" => fs::create_dir(&options.evidence).unwrap(),
            "symlink" => symlink(&foreign, &options.evidence).unwrap(),
            "dangling" => symlink(temp.path().join("missing"), &options.evidence).unwrap(),
            _ => {}
        }
        let mut peer = Peer {
            calls: 0,
            reject: 1,
            create_output: false,
        };
        assert!(run(&options, &mut peer, &AtomicBool::new(kind == "cancel")).is_err());
        assert_eq!(peer.calls, 0);
        assert_eq!(fs::read(foreign.join("sentinel")).unwrap(), b"preserve");
        if kind == "file" {
            assert_eq!(fs::read(&options.evidence).unwrap(), b"occupied");
        }
    }
}

#[test]
fn oracle_or_fixture_tampering_is_rejected_before_the_health_child() {
    for field in ["projection", "exception"] {
        let temp = tempfile::tempdir().unwrap();
        let mut options = setup(temp.path());
        let mut reference: Value =
            serde_json::from_slice(&fs::read(&options.reference).unwrap()).unwrap();
        if field == "projection" {
            reference["cases"][0]["projection"]["params"]["StoppingAccel"] = json!([48]);
        } else {
            reference["cases"][0]["source"]["exception"] = json!("");
        }
        fs::write(&options.reference, serde_json::to_vec(&reference).unwrap()).unwrap();
        options.reference_sha256 = openpilot_dev_tools::seal::digest(&options.reference).unwrap();
        let mut peer = Peer {
            calls: 0,
            reject: 1,
            create_output: false,
        };
        assert!(run(&options, &mut peer, &AtomicBool::new(false)).is_err());
        assert_eq!(peer.calls, 0);
    }
}

#[test]
fn actual_cli_int_and_term_stop_the_owned_trace_group_without_success() {
    use std::{
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for signal in [rustix::process::Signal::INT, rustix::process::Signal::TERM] {
        let temp = tempfile::tempdir().unwrap();
        let options = setup(temp.path());
        fs::write(&options.trace,br##"#!/bin/sh
input=$(cat)
case "$input" in
  *'"name":"health-HONDA_CIVIC-pid"'*) printf '%s\n' '[{"name":"health-HONDA_CIVIC-pid","rows":[{},{},{},{},{}]}]' > "$1"; exit 0;;
esac
sleep 120 &
printf '%s %s\n' "$$" "$!" > "$CONTROL_FAILURE_PEER_PIDS"
wait
"##).unwrap();
        fs::set_permissions(&options.trace, fs::Permissions::from_mode(0o700)).unwrap();
        let marker = temp.path().join("peer.pids");
        let mut child = ChildGuard(
            Command::new(env!("CARGO_BIN_EXE_check-control-failures-native"))
                .args(["--root"])
                .arg(&options.root)
                .arg("--trace")
                .arg(&options.trace)
                .arg("--numerics")
                .arg(&options.numerics)
                .arg("--reference")
                .arg(&options.reference)
                .arg("--reference-sha256")
                .arg(&options.reference_sha256)
                .arg("--evidence")
                .arg(&options.evidence)
                .env("CONTROL_FAILURE_PEER_PIDS", &marker)
                .stdout(Stdio::null())
                .stderr(Stdio::from(
                    fs::File::create(temp.path().join("cli.stderr")).unwrap(),
                ))
                .spawn()
                .unwrap(),
        );
        let until = Instant::now() + Duration::from_secs(10);
        while !marker.exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "CLI ended before live rejection peer; inspect cli.stderr"
            );
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(20));
        }
        let pids: Vec<i32> = fs::read_to_string(&marker)
            .unwrap()
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        rustix::process::kill_process(
            rustix::process::Pid::from_raw(child.0.id() as i32).unwrap(),
            signal,
        )
        .unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(20));
        };
        assert!(!status.success());
        assert!(!options.evidence.join("results.json").exists());
        for pid in pids {
            loop {
                let live = fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|text| {
                    text.rsplit_once(") ")
                        .is_some_and(|(_, tail)| !tail.starts_with('Z'))
                });
                if !live {
                    break;
                }
                assert!(Instant::now() < until, "owned peer remains alive");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

#[test]
fn replacing_published_evidence_preserves_foreign_directory_and_owned_diagnostics() {
    struct Replacing {
        peer: Peer,
        path: std::path::PathBuf,
        moved: std::path::PathBuf,
    }
    impl Runner for Replacing {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            let value = self.peer.run(spec)?;
            fs::rename(&self.path, &self.moved)?;
            fs::create_dir(&self.path)?;
            fs::write(self.path.join("sentinel"), b"foreign")?;
            Ok(value)
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let options = setup(temp.path());
    let moved = temp.path().join("moved-owned");
    let mut peer = Replacing {
        peer: Peer {
            calls: 0,
            reject: 1,
            create_output: false,
        },
        path: options.evidence.clone(),
        moved: moved.clone(),
    };
    assert!(run(&options, &mut peer, &AtomicBool::new(false)).is_err());
    assert_eq!(
        fs::read(options.evidence.join("sentinel")).unwrap(),
        b"foreign"
    );
    assert_eq!(fs::read_dir(&options.evidence).unwrap().count(), 1);
    assert!(!moved.join("results.json").exists());
    assert!(moved.join("health-HONDA_CIVIC-pid/input.json").is_file());
    // Insert a foreign CASE after the private inode is already held, before
    // publication. The actual production publish primitive must reject it.
    for kind in ["directory", "symlink", "file"] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("evidence");
        let owned = evidence_contract::Evidence::claim(&path).unwrap();
        let staged = owned.stage_case().unwrap();
        let published = path.join("construction-case");
        let foreign = temp.path().join("foreign-construction");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("sentinel"), b"preserve").unwrap();
        match kind {
            "directory" => {
                fs::create_dir(&published).unwrap();
                fs::write(published.join("sentinel"), b"preserve").unwrap();
            }
            "symlink" => std::os::unix::fs::symlink(&foreign, &published).unwrap(),
            _ => fs::write(&published, b"preserve").unwrap(),
        }
        assert!(staged.publish("construction-case").is_err());
        assert_eq!(fs::read(foreign.join("sentinel")).unwrap(), b"preserve");
        assert_eq!(fs::read_dir(&foreign).unwrap().count(), 1);
        if kind == "file" {
            assert_eq!(fs::read(&published).unwrap(), b"preserve");
        } else {
            assert_eq!(fs::read(published.join("sentinel")).unwrap(), b"preserve");
            assert_eq!(fs::read_dir(&published).unwrap().count(), 1);
        }
        assert!(!path.join("results.json").exists());
    }
    struct CaseReplacing {
        peer: Peer,
        root: std::path::PathBuf,
        foreign: std::path::PathBuf,
        displaced: std::path::PathBuf,
        alias: bool,
        timeout: bool,
    }
    impl Runner for CaseReplacing {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            if self.peer.calls == 0 {
                return self.peer.run(spec);
            }
            let value = self.peer.run(spec)?;
            let request: Value = serde_json::from_slice(spec.input.as_ref().unwrap())?;
            let published = self
                .root
                .join(request["cases"][0]["name"].as_str().unwrap());
            fs::rename(&published, &self.displaced)?;
            if self.alias {
                std::os::unix::fs::symlink(&self.foreign, &published)?;
            } else {
                fs::rename(&self.foreign, &published)?;
            }
            if self.timeout {
                return Err(Box::new(
                    openpilot_dev_tools::qa_process::CaptureError::Timeout {
                        timeout: spec.timeout,
                        stdout: b"partial-out".to_vec(),
                        stderr: b"partial-err".to_vec(),
                    },
                ));
            }
            Ok(value)
        }
    }
    for alias in [false, true] {
        for timeout in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let options = setup(temp.path());
            let foreign = temp.path().join("foreign-case");
            fs::create_dir(&foreign).unwrap();
            fs::write(foreign.join("sentinel"), b"foreign").unwrap();
            let displaced = temp.path().join("displaced-owned-case");
            let mut peer = CaseReplacing {
                peer: Peer {
                    calls: 0,
                    reject: 1,
                    create_output: false,
                },
                root: options.evidence.clone(),
                foreign: foreign.clone(),
                displaced: displaced.clone(),
                alias,
                timeout,
            };
            assert!(run(&options, &mut peer, &AtomicBool::new(false)).is_err());
            let foreign = if alias {
                foreign
            } else {
                options.evidence.join("malformed-StoppingAccel")
            };
            assert_eq!(fs::read(foreign.join("sentinel")).unwrap(), b"foreign");
            assert_eq!(fs::read_dir(foreign).unwrap().count(), 1);
            assert!(!options.evidence.join("results.json").exists());
            assert!(displaced.join("input.json").is_file());
            assert!(displaced.join("stdout.bin").is_file());
            assert!(displaced.join("stderr.bin").is_file());
            if timeout {
                assert_eq!(
                    fs::read(displaced.join("stdout.bin")).unwrap(),
                    b"partial-out"
                );
                assert_eq!(
                    fs::read(displaced.join("stderr.bin")).unwrap(),
                    b"partial-err"
                );
            } else {
                assert_eq!(
                    fs::read(displaced.join("stderr.bin")).unwrap(),
                    b"controlled rejection"
                );
            }
        }
    }
}
#[test]
fn timeout_is_not_mistaken_for_a_native_nonzero_exit_and_retains_partial_logs() {
    struct Timeout {
        peer: Peer,
    }
    impl Runner for Timeout {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            if self.peer.calls == 0 {
                return self.peer.run(spec);
            }
            Err(Box::new(
                openpilot_dev_tools::qa_process::CaptureError::Timeout {
                    timeout: spec.timeout,
                    stdout: b"partial-out".to_vec(),
                    stderr: b"partial-err".to_vec(),
                },
            ))
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let options = setup(temp.path());
    let mut peer = Timeout {
        peer: Peer {
            calls: 0,
            reject: 1,
            create_output: false,
        },
    };
    assert!(run(&options, &mut peer, &AtomicBool::new(false)).is_err());
    assert!(!options.evidence.join("results.json").exists());
    assert_eq!(
        fs::read(options.evidence.join("malformed-StoppingAccel/stdout.bin")).unwrap(),
        b"partial-out"
    );
    assert_eq!(
        fs::read(options.evidence.join("malformed-StoppingAccel/stderr.bin")).unwrap(),
        b"partial-err"
    );
}

#[test]
fn cli_unknown_duplicate_missing_value_and_bad_sha_leave_evidence_unclaimed() {
    use std::{ffi::OsString, process::Command};
    let temp = tempfile::tempdir().unwrap();
    let evidence = temp.path().join("evidence");
    let mut base: Vec<OsString> = Vec::new();
    for (flag, value) in [
        ("--root", temp.path().to_owned()),
        ("--trace", temp.path().join("never-executed")),
        ("--numerics", temp.path().join("numerics")),
        ("--reference", temp.path().join("original.json")),
        ("--evidence", evidence.clone()),
    ] {
        base.push(flag.into());
        base.push(value.into_os_string());
    }
    base.push("--reference-sha256".into());
    base.push("0".repeat(64).into());
    let mut variants = Vec::new();
    let mut unknown = base.clone();
    unknown.push("--unknown".into());
    variants.push(unknown);
    let mut duplicate = base.clone();
    duplicate.extend([OsString::from("--trace"), OsString::from("duplicate")]);
    variants.push(duplicate);
    let mut missing = base.clone();
    missing.push("--root".into());
    variants.push(missing);
    let mut bad = base;
    *bad.last_mut().unwrap() = "invalid".into();
    variants.push(bad);
    for args in variants {
        let output = Command::new(env!("CARGO_BIN_EXE_check-control-failures-native"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!evidence.exists());
    }
    assert!(
        Command::new(env!("CARGO_BIN_EXE_check-control-failures-native"))
            .arg("--help")
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(!evidence.exists());
}
