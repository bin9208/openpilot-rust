//! Full independently captured original alert-callbacks checker; generic policy remains live.
use crate::{
    alert_family_check::{self, Definition},
    qa_process::Runner,
    seal::Result,
};
use std::{io::Write, path::Path};
static DEFINITION: Definition = Definition {
    corpus: std::sync::OnceLock::new(),
    checker: "rust/tools/check_alert_callbacks.py",
    checker_sha: "5cbac01ade259ee1af080b240a2ce23c259ed982ec57ae6c8362b1222d68fbd5",
    manifest: include_str!("../tests/fixtures/alert-callbacks/manifest.json"),
    packed: include_bytes!("../tests/fixtures/alert-callbacks/corpus.json.gz"),
    live: &[
        "opendbc_repo/opendbc/car/car.capnp",
        "opendbc_repo/opendbc/car/hyundai/values.py",
        "openpilot/cereal/__init__.py",
        "openpilot/cereal/custom.capnp",
        "openpilot/cereal/deprecated.capnp",
        "openpilot/cereal/include/c++.capnp",
        "openpilot/cereal/log.capnp",
        "openpilot/common/constants.py",
        "openpilot/common/realtime.py",
        "openpilot/selfdrive/locationd/calibrationd.py",
        "openpilot/selfdrive/selfdrived/events.py",
        "openpilot/selfdrive/ui/feedback/feedbackd.py",
        "openpilot/selfdrive/ui/translations/app_de.po",
        "openpilot/selfdrive/ui/translations/app_en.po",
        "openpilot/selfdrive/ui/translations/app_es.po",
        "openpilot/selfdrive/ui/translations/app_fr.po",
        "openpilot/selfdrive/ui/translations/app_ja.po",
        "openpilot/selfdrive/ui/translations/app_ko.po",
        "openpilot/selfdrive/ui/translations/app_pt-BR.po",
        "openpilot/selfdrive/ui/translations/app_th.po",
        "openpilot/selfdrive/ui/translations/app_tr.po",
        "openpilot/selfdrive/ui/translations/app_uk.po",
        "openpilot/selfdrive/ui/translations/app_zh-CHS.po",
        "openpilot/selfdrive/ui/translations/app_zh-CHT.po",
        "openpilot/selfdrive/ui/translations/languages.json",
        "openpilot/system/micd.py",
        "openpilot/system/ui/lib/multilang.py",
        "rust/tools/generate_selfdrive_alerts.py",
    ],
    cases: 15876,
    timeout: 120,
    callbacks: true,
    command: "check-alert-callbacks",
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
