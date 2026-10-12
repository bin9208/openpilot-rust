use crate::{
    param_changes::History, param_restore::Restore, params::Backend, settings::Catalog, Error,
    Value,
};

const EXCLUDED: &[&str] = &[
    "CarName",
    "CarParams",
    "CarParamsCache",
    "CarParamsPersistent",
    "CarParamsPrevRoute",
    "CarModel",
    "CarFingerprint",
    "SupportedCars",
    "CompletedTrainingVersion",
    "TrainingVersion",
    "TermsVersion",
    "HasAcceptedTerms",
    "CalibrationParams",
    "LiveParameters",
    "LiveTorqueParameters",
    "DongleId",
    "HardwareSerial",
    "DeviceSerial",
    "DeviceType",
    "LanguageSetting",
    "IsMetric",
    "OpenpilotEnabledToggle",
    "ExperimentalMode",
    "ExperimentalModeConfirmed",
    "SshEnabled",
    "AdbEnabled",
    "GithubUsername",
    "GithubSshKeys",
    "RecordFront",
    "RecordAudio",
    "GitBranch",
    "GitCommit",
    "GitCommitDate",
    "UpdaterState",
    "UpdaterTargetBranch",
    "UpdaterCurrentDescription",
];
const PREFIXES: &[&str] = &[
    "CarParams",
    "Calibration",
    "CompletedTraining",
    "Git",
    "Github",
    "Updater",
    "Dongle",
    "Hardware",
    "DeviceSerial",
];

pub fn selected(name: &str, definition: &Value) -> bool {
    !name.is_empty()
        && matches!(definition, Value::Object(_))
        && definition.has("default")
        && !EXCLUDED.contains(&name)
        && !PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

pub fn reset(params: &mut Backend, catalog: &Catalog, history: &History) -> Result<Value, Error> {
    if !params.has_params() {
        return Err(Error::Source("params unavailable".into()));
    }
    let _fresh = super::fresh::reopen(params.native_params())?;
    let mut values = Vec::new();
    for (name, definition) in crate::json_fields::fields(&catalog.by_name)? {
        if selected(&Value::Text(name.clone()).string()?, definition) {
            values.push((name.clone(), definition.get("default").clone()));
        }
    }
    let restored = Restore::new(params, catalog, history).apply(
        &Value::Object(values),
        &Value::Null,
        &Value::text("reset_defaults"),
    )?;
    let ok = !restored.get("result").get("fail_cnt").truth();
    let entries = crate::json_fields::array(restored.get("preview").get("entries"))?;
    let applied = entries
        .iter()
        .filter(|entry| entry.get("apply").truth())
        .map(|entry| {
            Ok((
                crate::json_fields::key(entry.get("key"))?,
                entry.get("value").clone(),
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let message = if ok {
        "설정 초기화 성공"
    } else {
        "설정 초기화 실패"
    };
    let mut result = Value::object([
        ("ok", Value::Bool(ok)),
        ("message", Value::text(message)),
        (
            "error",
            if ok {
                Value::Null
            } else {
                Value::text(message)
            },
        ),
        ("values", Value::Object(applied)),
    ]);
    if let (Value::Object(result), Value::Object(restored)) = (&mut result, restored) {
        result.extend(restored);
    }
    Ok(result)
}
