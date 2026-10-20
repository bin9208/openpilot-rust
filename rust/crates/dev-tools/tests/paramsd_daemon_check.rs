#[path = "../src/paramsd_daemon_check.rs"]
mod controller;

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
fn unchanged_original_paramsd_controller_and_input_generator_are_pinned() {
    for (source, expected) in [
        (
            "rust/tools/check_paramsd_daemon.py",
            "04a5433f356d594d3db8ce42394274e0bb1c856d20e975c5657285ce8f998b87",
        ),
        (
            "rust/tools/paramsd_fixture.py",
            "8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521",
        ),
        (
            "rust/tools/paramsd_startup_phase.c",
            "1b524e78cdfa1cb4013a35cb570fd2fbc03cce7a0153a5ef8ba1627102265a3b",
        ),
    ] {
        assert_eq!(
            openpilot_dev_tools::seal::digest(&root().join(source)).unwrap(),
            expected
        );
    }
}

fn required(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")));
    assert!(path.is_absolute(), "{name} must be absolute");
    path
}

/// Run only after the unchanged Python checker has captured both input.json
/// files from the same sealed daemon/oracle source. This drives a new private
/// native namespace and retains all raw packets for the independent comparator.
#[test]
#[ignore = "requires paired original paramsd input captures and fresh source-built ELF/hook in Actions"]
fn full_continuous_and_restart_native_controller() {
    let options = controller::Options {
        daemon: required("PARAMSD_CONTROLLER_DAEMON"),
        library: required("PARAMSD_CONTROLLER_LIBRARY"),
        continuous_input: required("PARAMSD_CONTROLLER_CONTINUOUS_INPUT"),
        restart_input: required("PARAMSD_CONTROLLER_RESTART_INPUT"),
        evidence: required("PARAMSD_CONTROLLER_EVIDENCE"),
    };
    controller::run(&options).unwrap();
}
