#![cfg(all(target_os = "linux", feature = "controlsd-daemon-native"))]
// Synthetic packet mutations exercise controller contracts, never the oracle.
#[allow(dead_code)]
#[path = "../src/controlsd_daemon_check.rs"]
mod controller;
use controller::contract;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
fn pair(a: u64, b: u64) -> Vec<Vec<u8>> {
    let mut first = capnp::message::Builder::new_default();
    let mut event = first.init_root::<openpilot_cereal::log_capnp::event::Builder<'_>>();
    event.set_log_mono_time(a);
    event.init_controls_state().set_lateral_plan_mono_time(1);
    let mut second = capnp::message::Builder::new_default();
    let mut event = second.init_root::<openpilot_cereal::log_capnp::event::Builder<'_>>();
    event.set_log_mono_time(b);
    event.init_car_control();
    vec![
        capnp::serialize::write_message_to_words(&first),
        capnp::serialize::write_message_to_words(&second),
    ]
}
#[test]
fn exact_header_slots_pid_low_word_and_40_reader_limit() {
    let mut header = vec![0; 984];
    let put = |header: &mut [u8], i: usize, word: u64| {
        header[i * 8..i * 8 + 8].copy_from_slice(&word.to_le_bytes())
    };
    put(&mut header, 0, 40);
    put(&mut header, 82, 1);
    put(&mut header, 122, (55_u64 << 32) | 1234);
    assert!(contract::reader_header(&header, 1234).unwrap());
    assert!(!contract::reader_header(&header, 1235).unwrap());
    put(&mut header, 82, 0);
    assert!(!contract::reader_header(&header, 1234).unwrap());
    assert!(contract::reader_header(&header[..983], 1234).is_err());
    let mut writer = vec![0; 24];
    put(&mut writer, 2, (91_u64 << 32) | 1234);
    assert!(contract::writer_header(&writer, 1234).unwrap());
    assert!(!contract::writer_header(&writer, 91).unwrap());
}
#[test]
fn strict_packet_order_clock_trailing_bytes_and_input_marker() {
    let good = pair(50, 51);
    assert!(contract::pair(&good).unwrap().has_input);
    assert!(contract::pair(&pair(51, 50)).is_err());
    assert!(contract::pair(&[good[1].clone(), good[0].clone()]).is_err());
    assert!(contract::pair(&good[..1]).is_err());
    let mut bad = good;
    bad[0].push(0);
    assert!(contract::pair(&bad).is_err());
}
#[test]
fn complete_comparison_omits_only_publication_root_time_and_keeps_source_tolerance() {
    let raw = pair(50, 51);
    let values = contract::pair(&raw).unwrap().values;
    let packets = vec![raw; 140];
    let mut original = json!({"rows":vec![json!({"publications":values});140]});
    original["rows"][0]["publications"][0]["event"]["logMonoTime"] = json!(999);
    assert!(
        contract::compare_rows(&original, &packets)
            .unwrap()
            .numeric_comparisons
            > 0
    );
    for change in ["clock", "float", "extra", "rows", "publications"] {
        let mut bad = original.clone();
        match change {
            "clock" => {
                bad["rows"][0]["publications"][0]["event"]["controlsState"]["lateralPlanMonoTime"] =
                    json!(2)
            }
            "float" => {
                bad["rows"][0]["publications"][0]["event"]["controlsState"]["curvature"] = json!(1.)
            }
            "extra" => bad["rows"][0]["publications"][0]["event"]["foreign"] = json!(true),
            "rows" => {
                bad["rows"].as_array_mut().unwrap().pop();
            }
            _ => {
                bad["rows"][0]["publications"].as_array_mut().unwrap().pop();
            }
        }
        assert!(contract::compare_rows(&bad, &packets).is_err(), "{change}");
    }
}
#[test]
fn restart_params_allow_exactly_the_two_original_persisted_values() {
    let mut values = BTreeMap::from([
        ("LongitudinalPersonalityMax".into(), "3".into()),
        ("HyundaiCameraSccHint".into(), "0".into()),
    ]);
    contract::persisted(&values).unwrap();
    values.insert("NNFF".into(), "0".into());
    assert!(contract::persisted(&values).is_err());
    values.remove("NNFF");
    values.insert("HyundaiCameraSccHint".into(), "1".into());
    assert!(contract::persisted(&values).is_err());
}
#[test]
fn cli_rejects_missing_unknown_duplicate_and_backend_options_before_capture() {
    for args in [
        vec![],
        vec!["unknown"],
        vec!["capture", "--python", "x"],
        vec!["capture", "--root", "a", "--root", "b"],
        vec!["compare", "--root"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_check-controlsd-daemon-native"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
    }
}
#[test]
fn owned_child_leaf() {
    if let Some(path) = std::env::var_os("CONTROLSD_OWNED_LEAF") {
        fs::write(path, std::process::id().to_string()).unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}
#[test]
fn owned_worker_leaf() {
    if std::env::var_os("CONTROLSD_OWNED_WORKER").is_none() {
        return;
    }
    use openpilot_dev_tools::qa_process::LiveChild;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("pid");
    let pid;
    {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "owned_child_leaf", "--nocapture"])
            .env_remove("CONTROLSD_OWNED_WORKER")
            .env("CONTROLSD_OWNED_LEAF", &path);
        let mut child = LiveChild::spawn_in_owned_worker(&mut command).unwrap();
        pid = child.pid().unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        while !path.exists() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(child.poll().unwrap().is_none());
        child.signal(rustix::process::Signal::TERM).unwrap();
        while child.poll().unwrap().is_none() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(child.pid().is_err());
        assert!(child.signal(rustix::process::Signal::TERM).is_err());
    }
    assert!(!Path::new("/proc").join(pid.to_string()).exists());
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "owned_child_leaf", "--nocapture"])
        .env_remove("CONTROLSD_OWNED_WORKER")
        .env("CONTROLSD_OWNED_LEAF", &path);
    let child = LiveChild::spawn_in_owned_worker(&mut command).unwrap();
    let pid = child.pid().unwrap();
    drop(child);
    assert!(!Path::new("/proc").join(pid.to_string()).exists());
}
#[test]
fn real_shared_owner_drop_signal_and_reaped_identity_in_isolated_worker() {
    use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
    let mut spec = CommandSpec::new_os(
        vec![
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            "owned_worker_leaf".into(),
            "--nocapture".into(),
        ],
        Duration::from_secs(15),
    );
    spec.env.insert("CONTROLSD_OWNED_WORKER".into(), "1".into());
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        result.code,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
