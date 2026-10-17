//! Full independently captured original alert-manager checker; generic policy remains live.
use crate::{
    alert_family_check::{self, Definition},
    qa_process::Runner,
    seal::Result,
};
use std::{io::Write, path::Path};
static DEFINITION: Definition = Definition {
    corpus: std::sync::OnceLock::new(),
    checker: "rust/tools/check_alert_manager.py",
    checker_sha: "3fdea294820a41902d70e32852ecdf65897a385fec402e83bbaf4e65966b5705",
    manifest: include_str!("../tests/fixtures/alert-manager/manifest.json"),
    packed: include_bytes!("../tests/fixtures/alert-manager/corpus.json.gz"),
    live: &[
        "opendbc_repo/opendbc/car/car.capnp",
        "openpilot/cereal/__init__.py",
        "openpilot/cereal/custom.capnp",
        "openpilot/cereal/deprecated.capnp",
        "openpilot/cereal/include/c++.capnp",
        "openpilot/cereal/log.capnp",
        "openpilot/common/realtime.py",
        "openpilot/selfdrive/selfdrived/alertmanager.py",
        "openpilot/selfdrive/selfdrived/events.py",
    ],
    cases: 25000,
    timeout: 60,
    callbacks: false,
    command: "check-alert-manager",
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
