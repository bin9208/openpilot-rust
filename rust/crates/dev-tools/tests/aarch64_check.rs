#![cfg(feature = "aarch64-check-native")]
use capnp::message::Builder;
use openpilot_cereal::log_capnp::event;
use openpilot_dev_tools::aarch64_check as checker;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, os::unix::ffi::OsStringExt, path::Path};
fn fixture(name: &str) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aarch64-check");
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let bytes = fs::read(root.join(name)).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        manifest["fixtures"][name].as_str().unwrap()
    );
    serde_json::from_slice(&bytes).unwrap()
}
fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn synthetic_fixture_matches_original_make_process_bytes_and_links() {
    let root = tempfile::tempdir().unwrap();
    checker::prepare(root.path()).unwrap();
    let original = fixture("procfs.json");
    for (name, value) in original.as_object().unwrap() {
        let path = root.path().join(name);
        if let Some(link) = value["symlink"].as_str() {
            assert_eq!(fs::read_link(path).unwrap(), Path::new(link));
        } else {
            assert_eq!(
                fs::read(path).unwrap(),
                unhex(value["hex"].as_str().unwrap()),
                "{name}"
            );
        }
    }
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 3);
}
#[test]
fn every_original_frame_assertion_and_only_those_fields_are_checked() {
    let bytes = unhex(fixture("original-event.json")["wire"].as_str().unwrap());
    checker::check_frame(&bytes).unwrap();
    let rows = fixture("original-controls.json");
    for case in [
        "invalid",
        "wrong-union",
        "mem-total",
        "cpu-count",
        "proc-count",
        "pid",
        "memRss",
        "memPss",
        "nice",
        "name",
    ] {
        let source = capnp::serialize::read_message(bytes.as_slice(), Default::default()).unwrap();
        let mut b = Builder::new_default();
        b.set_root::<event::Owned>(source.get_root::<event::Reader<'_>>().unwrap())
            .unwrap();
        let mut e = b.get_root::<event::Builder<'_>>().unwrap();
        match case {
            "invalid" => e.set_valid(false),
            "wrong-union" => {
                e.set_log_message("unrelated");
            }
            _ => {
                let event::ProcLog(log) = e.reborrow().which().unwrap() else {
                    panic!("not procLog")
                };
                let mut log = log.unwrap();
                match case {
                    "mem-total" => log.reborrow().get_mem().unwrap().set_total(0),
                    "cpu-count" => {
                        log.reborrow().init_cpu_times(0);
                    }
                    "proc-count" => {
                        log.reborrow().init_procs(0);
                    }
                    _ => {
                        let mut p = log.get_procs().unwrap().get(0);
                        match case {
                            "pid" => p.set_pid(124),
                            "memRss" => p.set_mem_rss(8192001),
                            "memPss" => p.set_mem_pss(51201),
                            "nice" => p.set_nice(-4),
                            "name" => p.set_name("other"),
                            _ => unreachable!(),
                        }
                    }
                }
            }
        }
        let expected = rows
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["case"] == case)
            .unwrap();
        assert_eq!(
            checker::check_frame(&capnp::serialize::write_message_to_words(&b)).is_ok(),
            expected["accepted"].as_bool().unwrap(),
            "{case}"
        );
    }
    let source = capnp::serialize::read_message(bytes.as_slice(), Default::default()).unwrap();
    let mut b = Builder::new_default();
    b.set_root::<event::Owned>(source.get_root::<event::Reader<'_>>().unwrap())
        .unwrap();
    let mut e = b.get_root::<event::Builder<'_>>().unwrap();
    e.set_log_mono_time(12345);
    let event::ProcLog(log) = e.which().unwrap() else {
        panic!("not procLog")
    };
    log.unwrap().get_mem().unwrap().set_free(777);
    checker::check_frame(&capnp::serialize::write_message_to_words(&b)).unwrap();
}
#[test]
fn report_keeps_exact_hash_and_truthful_nondevice_scope() {
    let tmp = tempfile::tempdir().unwrap();
    let binary = tmp.path().join("binary");
    fs::write(&binary, b"original-controller-pinned-input\0\xff").unwrap();
    let rows = fixture("original-controls.json");
    let expected = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["case"] == "exact")
        .unwrap();
    assert_eq!(checker::report(&binary, 3).unwrap(), expected["report"]);
    assert_eq!(expected["report"]["device_validation"], json!("not_run"));
}
#[test]
fn cli_requires_all_three_paths_and_preserves_raw_bytes() {
    for args in [
        vec![],
        vec!["--binary"],
        vec!["--binary", "x", "--runner", "r"],
        vec!["--unknown", "x"],
    ] {
        assert!(
            checker::arguments(&args.into_iter().map(OsString::from).collect::<Vec<_>>()).is_err()
        );
    }
    assert!(checker::arguments(&["--help".into()]).unwrap().is_none());
    let raw = OsString::from_vec(b"native-\xff".to_vec());
    let options = checker::arguments(&[
        "--binary".into(),
        raw.clone(),
        "--runner".into(),
        "qemu".into(),
        "--report".into(),
        "report.json".into(),
    ])
    .unwrap()
    .unwrap();
    assert_eq!(options.binary, Path::new(&raw));
    assert_eq!(options.runner, Path::new("qemu"));
    assert_eq!(options.report, Path::new("report.json"));
}

#[test]
fn native_elf_inspection_preserves_original_readelf_architecture_and_static_checks() {
    let rows = fixture("elf-controls.json");
    for row in rows.as_array().unwrap() {
        assert_eq!(
            checker::inspect_static(&unhex(row["hex"].as_str().unwrap())).is_ok(),
            row["original_readelf_admitted"].as_bool().unwrap(),
            "{}",
            row["case"]
        );
    }
    for bytes in [vec![], b"not an ELF".to_vec()] {
        assert!(checker::inspect_static(&bytes).is_err());
    }
}

#[cfg(target_os = "linux")]
#[path = "aarch64_check/process.rs"]
mod process;
