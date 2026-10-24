#![cfg(feature = "managed-entry-native")]
use openpilot_dev_tools::managed_entry_check::{cases, normalize, parse, Command};
use serde_json::{json, Value};
use std::{collections::BTreeMap, ffi::OsString, os::unix::ffi::OsStringExt};

#[test]
fn unknown_case_cannot_succeed_with_zero_comparisons() {
    let cwd = tempfile::tempdir().unwrap();
    let args = [
        "/bin/true",
        "/bin/true",
        "binding",
        "output",
        "--case",
        "nonexistent-control",
    ]
    .map(OsString::from);
    let error = parse(&args, cwd.path(), &BTreeMap::new()).expect_err("unknown case accepted");
    assert!(error.to_string().contains("unknown managed-entry case"));
}

#[test]
fn original_roster_preserves_all_twenty_two_scenarios_and_explicit_selection() {
    let captured: Value =
        serde_json::from_str(include_str!("fixtures/managed-entry/original-fields.json")).unwrap();
    let all = cases();
    assert_eq!(all.len(), 22);
    for (name, scenario) in &all {
        let row = captured["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["case"] == *name)
            .unwrap();
        let mut expected = row["request"].clone();
        expected.as_object_mut().unwrap().remove("process");
        if *name == "name-error" {
            expected["process"] = row["request"]["process"].clone();
        }
        expected.as_object_mut().unwrap().remove("daemon");
        if ["unicode-daemon", "global-context-precedence"].contains(name) {
            expected["daemon"] = row["request"]["daemon"].clone();
        }
        assert_eq!(scenario, &expected, "{name}");
        let args = ["binary", "collector", "binding", "output", "--case", name].map(OsString::from);
        let Command::Run(options) =
            parse(&args, std::path::Path::new("/tmp"), &BTreeMap::new()).unwrap()
        else {
            panic!("unexpected help");
        };
        assert_eq!(options.case.as_deref(), Some(*name));
    }
}

#[test]
fn raw_paths_runner_remainder_and_help_do_not_require_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let raw = OsString::from_vec(b"binary-\xff".to_vec());
    let args = [
        raw.clone(),
        "collector".into(),
        "binding".into(),
        "output".into(),
        "--c=normal".into(),
        "--run".into(),
        "wrapper".into(),
        "--help".into(),
        raw.clone(),
    ];
    let Command::Run(options) = parse(&args, root.path(), &BTreeMap::new()).unwrap() else {
        panic!("runner help escaped remainder");
    };
    assert_eq!(options.binary, root.path().join(&raw));
    assert_eq!(
        options.runner,
        vec![OsString::from("wrapper"), OsString::from("--help"), raw]
    );
    assert_eq!(options.case.as_deref(), Some("normal"));
    assert!(matches!(
        parse(&["--help".into()], root.path(), &BTreeMap::new()).unwrap(),
        Command::Help
    ));
    for count in 0..4 {
        assert!(parse(&args[..count], root.path(), &BTreeMap::new()).is_err());
    }
}

#[test]
fn normalization_preserves_order_values_and_params_while_removing_only_original_diagnostics() {
    let input = json!([{"op":"daemon_tag","fields":{"key":"daemon","value_json":"\"데몬😀\""},"carrot_exception":"previous","comm":"native"}, {"op":"capture_exception","fields":{"kind":"native-kind","message":"detail"},"carrot_exception":"error"}, {"step":"body","prepared":"data","secondary":{"arbitrary":true}}]);
    assert_eq!(
        normalize(&input).unwrap(),
        json!([{"op":"daemon_tag","fields":{"key":"daemon","value_json":"데몬😀"},"carrot_exception":"previous"}, {"op":"capture_exception","fields":{"captured_exception":true},"carrot_exception":"error"}, {"step":"body","prepared":"data"}])
    );
    assert!(normalize(&json!({"value_json":"not JSON"})).is_err());
}

// Comparator unit input, deliberately assembled from the independently captured
// source fields. This is not a live candidate/source parity result.
fn comparison_pair(name: &str) -> (Value, Value, Value) {
    let captured: Value =
        serde_json::from_str(include_str!("fixtures/managed-entry/original-fields.json")).unwrap();
    let row = captured["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["case"] == name)
        .unwrap();
    let scenario = cases().into_iter().find(|(key, _)| *key == name).unwrap().1;
    let process = row["request"]["process"].as_str().unwrap();
    let mut source = json!({"exit_code":row["exit_code"], "params":row["params"], "ready":row["ready"], "response":{"outcome":row["outcome"],"steps":row["steps"],"sdk_calls":row["sdk_calls"],"comm":process.chars().take(15).collect::<String>(),"cmdline":process},"records":row["records"],"captures":[]});
    for topic in ["logMessage", "errorLogMessage"] {
        for record in source["records"][topic].as_array_mut().unwrap() {
            let exception = record
                .as_object_mut()
                .unwrap()
                .remove("has_exception")
                .unwrap();
            if exception == true {
                record["exc_info"] = json!("original diagnostic");
            }
        }
    }
    // normalize() expects the raw SDK value_json field, not the already decoded
    // representation saved in original-fields.json.
    for call in source["response"]["sdk_calls"].as_array_mut().unwrap() {
        if let Some(fields) = call["fields"].as_object_mut() {
            if let Some(value) = fields.get_mut("value_json") {
                *value = json!(value.to_string());
            }
        }
    }
    for capture in row["captures"].as_array().unwrap() {
        let mut event = capture.clone();
        event["exception"] = json!({"values":[{"type":"fixture"}]});
        source["captures"]
            .as_array_mut()
            .unwrap()
            .push(json!({"event":event}));
    }
    let mut native = source.clone();
    native["response"]["cmdline"] = json!("/native/examples/managed_child request.json ");
    native["response"]["outcome"]["exception"] =
        json!({"kind":"fixture", "message":"raised", "backtrace":"frame at fixture.rs:1"});
    for topic in ["logMessage", "errorLogMessage"] {
        for record in native["records"][topic].as_array_mut().unwrap() {
            record["ctx"]["runtime_language"] = json!("rust");
            record["ctx"]["source_commit"] = json!("source");
            record["ctx"]["source_tree"] = json!("tree");
            if record["msg"] == "crash" {
                record["exc_info"] = json!("fixture: raised\nframe at fixture.rs:1");
            }
        }
    }
    for capture in native["captures"].as_array_mut().unwrap() {
        capture["event"]["platform"] = json!("native");
        capture["event"]["extra"] = json!({"rust_backtrace":"frame at fixture.rs:1"});
    }
    (source, native, scenario)
}

#[test]
fn comparison_preserves_the_original_twenty_two_policy_contracts() {
    for (name, _) in cases() {
        let (source, native, scenario) = comparison_pair(name);
        let report = openpilot_dev_tools::managed_entry_check::compare(&source, &native, &scenario)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(report["result"], "PASS");
    }
}

#[test]
fn comparator_rejects_real_contract_mutations_instead_of_only_counting_cases() {
    use openpilot_dev_tools::managed_entry_check::compare;
    let (source, native, scenario) = comparison_pair("error");
    for (pointer, value) in [
        ("/exit_code", json!(0)),
        ("/params", json!("wrong")),
        ("/ready/reporting_enabled", json!(false)),
        ("/response/outcome/stage", json!("Prepare")),
        ("/response/steps/0/step", json!("body")),
        ("/response/sdk_calls/0/carrot_exception", json!("wrong")),
        ("/records/logMessage/0/ctx/daemon", json!("wrong")),
        (
            "/records/logMessage/0/ctx/runtime_language",
            json!("python"),
        ),
        (
            "/records/errorLogMessage/0/exc_info",
            json!("different exception"),
        ),
        (
            "/response/outcome/exception/backtrace",
            json!("disabled backtrace"),
        ),
        ("/captures/0/event/tags/daemon", json!("wrong")),
        ("/captures/0/event/platform", json!("python")),
        ("/captures/0/event/exception/values", json!([])),
        ("/captures/0/event/extra/rust_backtrace", json!("")),
        ("/response/comm", json!("wrong")),
        (
            "/response/cmdline",
            json!("openpilot.fixture_managed_entry "),
        ),
    ] {
        let mut changed = native.clone();
        *changed
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("missing fixture pointer {pointer}")) = value;
        assert!(
            compare(&source, &changed, &scenario).is_err(),
            "accepted mutation {pointer}"
        );
    }
    let mut changed = native.clone();
    changed["response"]["sdk_calls"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(compare(&source, &changed, &scenario).is_err());
    for key in ["logMessage", "errorLogMessage"] {
        let mut changed = native.clone();
        changed["records"][key].as_array_mut().unwrap().pop();
        assert!(compare(&source, &changed, &scenario).is_err());
    }
}

#[test]
fn readiness_parser_has_only_original_four_positionals_and_help() {
    use openpilot_dev_tools::managed_entry_check::parse_readiness;
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/managed-entry/readiness-cli-original.json"
    ))
    .unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let base = ["binary", "collector", "binding", "output"].map(OsString::from);
    for row in original["controls"].as_array().unwrap() {
        let extra = row["extra"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| OsString::from(arg.as_str().unwrap()))
            .collect::<Vec<_>>();
        if row["case"] == "help" {
            assert_eq!(row["exit"], 0);
            assert!(matches!(
                parse_readiness(&extra, cwd.path(), &BTreeMap::new()).unwrap(),
                Command::Help
            ));
        } else {
            assert_eq!(row["exit"], 2);
            let args = base.iter().cloned().chain(extra).collect::<Vec<_>>();
            assert!(
                parse_readiness(&args, cwd.path(), &BTreeMap::new()).is_err(),
                "{}",
                row["case"]
            );
        }
    }
    let raw = OsString::from_vec(b"raw-\xff".to_vec());
    let args = [
        OsString::from("--"),
        OsString::from("-binary"),
        raw.clone(),
        OsString::from("binding"),
        OsString::from("output"),
    ];
    let Command::Run(options) = parse_readiness(&args, cwd.path(), &BTreeMap::new()).unwrap()
    else {
        panic!("raw positional rejected");
    };
    assert_eq!(options.binary, cwd.path().join("-binary"));
    assert_eq!(options.collector, cwd.path().join(raw));
    assert!(options.case.is_none());
    assert!(options.runner.is_empty());
}
