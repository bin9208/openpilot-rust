//! Full independently captured original selfdrive-events checker; generic policy remains live.
use crate::{
    alert_family_check::{self, Definition},
    qa_process::Runner,
    seal::Result,
};
use std::{io::Write, path::Path};
static DEFINITION: Definition = Definition {
    corpus: std::sync::OnceLock::new(),
    checker: "rust/tools/check_selfdrive_events.py",
    checker_sha: "acd7621784acb3743b33efa3514c1dae4e44396587cc52c20fe1e51edc82876d",
    manifest: include_str!("../tests/fixtures/selfdrive-events/manifest.json"),
    packed: include_bytes!("../tests/fixtures/selfdrive-events/corpus.json.gz"),
    live: &[
        "opendbc_repo/opendbc/car/car.capnp",
        "openpilot/cereal/__init__.py",
        "openpilot/cereal/custom.capnp",
        "openpilot/cereal/deprecated.capnp",
        "openpilot/cereal/include/c++.capnp",
        "openpilot/cereal/log.capnp",
        "openpilot/common/realtime.py",
        "openpilot/selfdrive/selfdrived/events.py",
        "rust/tools/generate_selfdrive_alerts.py",
    ],
    cases: 4872,
    timeout: 60,
    callbacks: false,
    command: "check-selfdrive-events",
};
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    alert_family_check::check(root, binary, output, runner, printed, &DEFINITION)
}
pub fn cli() -> Result<()> {
    alert_family_check::cli(&DEFINITION)
}
