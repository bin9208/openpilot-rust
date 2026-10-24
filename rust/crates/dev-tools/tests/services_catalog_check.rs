//! Expectations captured from the unchanged original before native implementation.
use openpilot_dev_tools::services_catalog_check::{self as native, Service};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/services-catalog")
        .join(name)
}
fn json(name: &str) -> Value {
    serde_json::from_slice(&fs::read(fixture(name)).unwrap()).unwrap()
}
fn catalog() -> Vec<Service> {
    serde_json::from_value(json("catalog.json")).unwrap()
}
#[test]
fn all_original_service_values_order_and_exact_generated_bytes() {
    let rows = catalog();
    assert_eq!(rows.len(), 88);
    let old: Value = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/native-build-closure/services.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let fresh = json("catalog.json");
    for (actual, expected) in fresh
        .as_array()
        .unwrap()
        .iter()
        .zip(old.as_array().unwrap())
    {
        // Original source stores navModel/mapRenderState decimation as 4.0/1.0;
        // the checker itself applies int(). Preserve that independently captured
        // distinction while checking all remaining tuple fields exactly.
        assert_eq!(
            actual["decimation"].as_f64(),
            expected["decimation"].as_f64()
        );
        let mut actual = actual.clone();
        let mut expected = expected.clone();
        actual.as_object_mut().unwrap().remove("decimation");
        expected.as_object_mut().unwrap().remove("decimation");
        assert_eq!(actual, expected);
    }
    assert_eq!(
        native::render(&rows).unwrap(),
        fs::read_to_string(fixture("generated.rs")).unwrap()
    );
    assert_eq!(native::catalog().unwrap(), rows);
    for row in &rows {
        assert!(row.frequency <= 104.);
        assert_ne!(row.decimation, Some(0));
    }
    let captured = json("schema.json");
    let fields = native::schema_fields().unwrap();
    assert_eq!(
        fields,
        captured["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
}
#[test]
fn independent_generic_float_repr_casts_ranges_and_insertion_order() {
    let cases = json("format-original.json");
    assert_eq!(cases.as_array().unwrap().len(), 10);
    for case in cases.as_array().unwrap() {
        let rows: Vec<Service> = serde_json::from_value(case["catalog"].clone()).unwrap();
        assert_eq!(
            native::render(&rows).unwrap(),
            case["generated"].as_str().unwrap()
        );
    }
    let mut rows = catalog();
    rows.swap(0, 1);
    assert_ne!(
        native::render(&rows).unwrap(),
        fs::read_to_string(fixture("generated.rs")).unwrap()
    );
}
#[test]
fn original_schema_admission_and_emit_bypass_preserve_failure_order() {
    let cases = json("schema-original.json");
    assert_eq!(cases.as_array().unwrap().len(), 7);
    for case in cases.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp
            .path()
            .join("rust/crates/messaging/src/services_generated.rs");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, case["candidate"].as_str().unwrap()).unwrap();
        let fields: Vec<String> = serde_json::from_value(case["fields"].clone()).unwrap();
        let actual = native::check_with(
            temp.path(),
            case["emit"].as_bool().unwrap(),
            &catalog(),
            &fields,
        );
        if case["error"].is_null() {
            assert_eq!(actual.unwrap(), case["stdout"].as_str().unwrap());
        } else {
            assert_eq!(
                actual.unwrap_err().to_string(),
                case["error"]["message"].as_str().unwrap()
            );
        }
    }
}
#[test]
fn genuine_original_file_failures_and_all_six_field_mutations() {
    let schema: Vec<String> =
        serde_json::from_value(json("schema.json")["fields"].clone()).unwrap();
    let cases = json("process-original.json");
    for case in cases.as_array().unwrap().iter().filter(|v| {
        v["args"].as_array().unwrap().is_empty()
            || matches!(v["name"].as_str(), Some("emit-missing" | "emit-wrong"))
    }) {
        let temp = tempfile::tempdir().unwrap();
        let target = temp
            .path()
            .join("rust/crates/messaging/src/services_generated.rs");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let kind = case["candidate"].as_str().unwrap();
        match kind {
            "directory" => fs::create_dir(&target).unwrap(),
            "missing" => (),
            v => fs::write(&target, hex(v)).unwrap(),
        }
        let emit = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a == "--emit-rust");
        let actual = native::check_with(temp.path(), emit, &catalog(), &schema);
        if case["code"] == 0 {
            assert_eq!(actual.unwrap(), case["stdout"].as_str().unwrap());
        } else if case["stderr"].as_str().unwrap().contains("AssertionError:") {
            assert_eq!(
                actual.unwrap_err().to_string(),
                "Rust service catalog differs from source; regenerate with --emit-rust"
            );
        } else {
            let error = actual.unwrap_err();
            let io = error
                .downcast_ref::<std::io::Error>()
                .expect("original filesystem/UTF8 failure remains an IO failure");
            match case["name"].as_str().unwrap() {
                "missing" => assert_eq!(io.kind(), std::io::ErrorKind::NotFound),
                "directory" => assert_eq!(io.kind(), std::io::ErrorKind::IsADirectory),
                "invalid-utf8" => assert_eq!(io.kind(), std::io::ErrorKind::InvalidData),
                name => panic!("uncaptured failure phase {name}: {error}"),
            }
        }
    }
}
fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn required_source_closure_detects_every_mutation_and_only_checker_may_retire() {
    let manifest = json("manifest.json");
    let original = root();
    native::validate_manifest(&manifest).unwrap();
    for section in ["sources", "corpora"] {
        for key in manifest[section].as_object().unwrap().keys() {
            let mut mutated = manifest.clone();
            mutated[section].as_object_mut().unwrap().remove(key);
            assert!(
                native::validate_manifest(&mutated).is_err(),
                "missing required {section} key {key}"
            );
        }
        let mut mutated = manifest.clone();
        mutated[section]["unexpected-pin"] = Value::String("bad".into());
        assert!(
            native::validate_manifest(&mutated).is_err(),
            "unexpected {section} key"
        );
    }
    let temp = tempfile::tempdir().unwrap();
    let repr_manifest = json("cli-repr-manifest.json");
    let paths = manifest["sources"]
        .as_object()
        .unwrap()
        .keys()
        .chain(repr_manifest["format_sources"].as_object().unwrap().keys())
        .cloned()
        .collect::<Vec<_>>();
    for path in paths
        .iter()
        .map(String::as_str)
        .chain(std::iter::once("uv.lock"))
    {
        let target = temp.path().join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match fs::copy(original.join(path), target) {
            Ok(_) => (),
            Err(error)
                if path == "rust/tools/check_services.py"
                    && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("copy mandatory {path}: {error}"),
        }
    }
    native::validate_source(temp.path()).unwrap();
    for path in &paths {
        let target = temp.path().join(path);
        let old = fs::read(&target).ok();
        fs::write(&target, b"source mutation\n").unwrap();
        assert!(
            native::validate_source(temp.path()).is_err(),
            "changed {path}"
        );
        fs::remove_file(&target).unwrap();
        assert_eq!(
            native::validate_source(temp.path()).is_ok(),
            path == "rust/tools/check_services.py",
            "absent {path}"
        );
        if let Some(old) = old {
            fs::write(target, old).unwrap();
        }
    }
    let lock = temp.path().join("uv.lock");
    let old = fs::read_to_string(&lock).unwrap();
    fs::write(
        &lock,
        old.replace("name = \"pycapnp\"", "name = \"removed-pycapnp\""),
    )
    .unwrap();
    assert!(native::validate_source(temp.path()).is_err());
    for (name, expected) in manifest["corpora"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(fixture(name)).unwrap())),
            expected.as_str().unwrap()
        );
    }
    for (name, expected) in repr_manifest["corpora"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(fixture(name)).unwrap())),
            expected.as_str().unwrap()
        );
    }
}
#[test]
fn actual_native_cli_matches_independent_original_help_flags_and_errors_without_python() {
    let binary = env!("CARGO_BIN_EXE_check-services");
    let cases = json("process-original.json");
    let normalize = |value: &str| {
        value
            .replace("check_services.py", "$PROGRAM")
            .replace("check-services", "$PROGRAM")
    };
    let empty = tempfile::tempdir().unwrap();
    for case in cases.as_array().unwrap().iter().take(14) {
        let args: Vec<String> = serde_json::from_value(case["args"].clone()).unwrap();
        let result = std::process::Command::new(binary)
            .args(args)
            .env("PATH", empty.path())
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(case["code"].as_i64().unwrap() as i32),
            "{case}"
        );
        assert_eq!(
            normalize(std::str::from_utf8(&result.stdout).unwrap()),
            normalize(case["stdout"].as_str().unwrap()),
            "{case}"
        );
        assert_eq!(
            normalize(std::str::from_utf8(&result.stderr).unwrap()),
            normalize(case["stderr"].as_str().unwrap()),
            "{case}"
        );
    }
}
#[test]
fn original_explicit_value_repr_preserves_quotes_controls_unicode_and_unix_bytes() {
    use std::os::unix::ffi::OsStringExt;
    let bytes = fs::read(fixture("cli-repr-original.json")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "27496943acea044b1a8f8a7573a08ba50d2bf1f8c1d1392d9f7c5b5321730167"
    );
    let cases: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 18);
    let empty = tempfile::tempdir().unwrap();
    for case in cases.as_array().unwrap() {
        let arguments = case["argv_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| std::ffi::OsString::from_vec(hex(v.as_str().unwrap())))
            .collect::<Vec<_>>();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-services"))
            .args(arguments)
            .env("PATH", empty.path())
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(case["code"].as_i64().unwrap() as i32),
            "{case}"
        );
        assert_eq!(
            std::str::from_utf8(&result.stdout).unwrap(),
            case["stdout"].as_str().unwrap(),
            "{case}"
        );
        assert_eq!(
            std::str::from_utf8(&result.stderr)
                .unwrap()
                .replace("check-services", "$PROGRAM"),
            case["stderr"].as_str().unwrap(),
            "{case}"
        );
    }
}
