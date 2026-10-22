#![cfg(target_os = "linux")]

use openpilot_dev_tools::{
    manager_cars_generate::BRANDS,
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    os::unix::fs::{symlink, MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};

fn required(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")))
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Copy)]
struct Inputs<'a> {
    binary: &'a Path,
    root: &'a Path,
    evidence: &'a Path,
}

fn invoke(inputs: Inputs<'_>, output: &Path, extra: &[OsString], name: &str, code: i32) -> Vec<u8> {
    let mut argv = vec![
        inputs.binary.as_os_str().to_os_string(),
        output.as_os_str().to_os_string(),
    ];
    argv.extend_from_slice(extra);
    let mut command = CommandSpec::new_os(argv, Duration::from_secs(30));
    command.cwd = Some(inputs.root.to_path_buf());
    command.env_remove = vec!["PYTHONPATH".into(), "PYTHONHOME".into()];
    let mut runner = NativeRunner;
    let captured = runner.run(&command).unwrap();
    fs::write(
        inputs.evidence.join(format!("{name}.stdout")),
        &captured.stdout,
    )
    .unwrap();
    fs::write(
        inputs.evidence.join(format!("{name}.stderr")),
        &captured.stderr,
    )
    .unwrap();
    fs::write(
        inputs.evidence.join(format!("{name}.status")),
        format!("{}\n", captured.code),
    )
    .unwrap();
    assert_eq!(captured.code, code, "{name} failed");
    captured.stdout
}

#[test]
#[ignore = "hosted unchanged Python export and native same-input comparison"]
fn fresh_original_car_docs_match_native_generation_and_check_mode() {
    let root = required("MANAGER_CARS_SOURCE_ROOT").canonicalize().unwrap();
    let original = required("MANAGER_CARS_ORIGINAL_OUTPUT");
    let original_stdout = required("MANAGER_CARS_ORIGINAL_STDOUT");
    let binary = required("MANAGER_CARS_NATIVE_BINARY")
        .canonicalize()
        .unwrap();
    let binding = required("MANAGER_CARS_BINDING").canonicalize().unwrap();
    let evidence = required("MANAGER_CARS_EVIDENCE");
    assert!(!evidence.exists());
    fs::create_dir(&evidence).unwrap();
    let held_binary = fs::read(&binary).unwrap();
    let held_binding = fs::read(&binding).unwrap();
    let binding_meta = fs::symlink_metadata(&binding).unwrap();
    assert!(binding_meta.is_file() && !binding_meta.file_type().is_symlink());
    assert!(held_binding.len() >= 20 && &held_binding[..4] == b"\x7fELF");
    let binding_identity = (
        binding_meta.dev(),
        binding_meta.ino(),
        binding_meta.len(),
        binding_meta.mode(),
        (binding_meta.mtime(), binding_meta.mtime_nsec()),
        (binding_meta.ctime(), binding_meta.ctime_nsec()),
    );
    let inputs = Inputs {
        binary: &binary,
        root: &root,
        evidence: &evidence,
    };
    let source_before = BRANDS
        .iter()
        .map(|brand| {
            let path = root.join("opendbc/car").join(brand).join("values.py");
            (path.clone(), sha(&fs::read(path).unwrap()))
        })
        .chain(
            ["opendbc/car/__init__.py", "opendbc/car/docs_definitions.py"].map(|name| {
                let path = root.join(name);
                (path.clone(), sha(&fs::read(path).unwrap()))
            }),
        )
        .collect::<Vec<_>>();
    let output = evidence.join("native");
    let stdout = invoke(inputs, &output, &[], "generate", 0);
    assert_eq!(stdout, fs::read(&original_stdout).unwrap());
    for brand in BRANDS {
        let original_bytes = fs::read(original.join(format!("{brand}.json"))).unwrap();
        let native_bytes = fs::read(output.join(format!("{brand}.json"))).unwrap();
        let prior = fs::read(
            root.join("rust/crates/manager/data/cars")
                .join(format!("{brand}.json")),
        )
        .unwrap();
        assert_eq!(
            native_bytes, original_bytes,
            "fresh source car names changed: {brand}"
        );
        assert_eq!(
            original_bytes, prior,
            "original current source differs from installed manager data: {brand}"
        );
        let names: serde_json::Value = serde_json::from_slice(&native_bytes).unwrap();
        let names = names.as_array().unwrap();
        assert!(!names.is_empty() && names.iter().all(serde_json::Value::is_string));
    }
    let check = invoke(inputs, &output, &[OsString::from("--check")], "check", 0);
    assert_eq!(check, stdout);
    let changed = output.join("ford.json");
    fs::write(&changed, b"[\"wrong\"]\n").unwrap();
    invoke(
        inputs,
        &output,
        &[OsString::from("--check")],
        "changed-check",
        1,
    );
    assert_eq!(fs::read(&changed).unwrap(), b"[\"wrong\"]\n");
    let symlink_target = evidence.join("gm-target.json");
    fs::rename(output.join("gm.json"), &symlink_target).unwrap();
    symlink(&symlink_target, output.join("gm.json")).unwrap();
    let symlink_stdout = invoke(inputs, &output, &[], "symlink-output", 0);
    assert_eq!(symlink_stdout, stdout);
    assert!(fs::symlink_metadata(output.join("gm.json"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read(&symlink_target).unwrap(),
        fs::read(original.join("gm.json")).unwrap()
    );
    let missing_check = evidence.join("missing-check-output");
    invoke(
        inputs,
        &missing_check,
        &[OsString::from("--check")],
        "missing-check",
        1,
    );
    assert!(missing_check.is_dir());
    assert!(!missing_check.join("hyundai.json").exists());
    let unsupported_output = evidence.join("unsupported-binding-output");
    invoke(
        inputs,
        &unsupported_output,
        &[
            OsString::from("--binding"),
            binding.as_os_str().to_os_string(),
        ],
        "unsupported-binding-option",
        2,
    );
    assert!(
        !unsupported_output.exists(),
        "unsupported option created output"
    );
    let changed_root = evidence.join("changed-source-root");
    for (path, _) in &source_before {
        let relative = path.strip_prefix(&root).unwrap();
        let target = changed_root.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(path, target).unwrap();
    }
    let changed_definition = changed_root.join("opendbc/car/docs_definitions.py");
    let mut changed_bytes = fs::read(&changed_definition).unwrap();
    changed_bytes.extend_from_slice(b"\n# source revision requires review\n");
    fs::write(&changed_definition, changed_bytes).unwrap();
    let changed_output = evidence.join("changed-source-output");
    invoke(
        Inputs {
            root: &changed_root,
            ..inputs
        },
        &changed_output,
        &[],
        "changed-source",
        1,
    );
    assert!(!changed_output.exists(), "changed source created output");
    assert_eq!(sha(&fs::read(&binary).unwrap()), sha(&held_binary));
    assert_eq!(sha(&fs::read(&binding).unwrap()), sha(&held_binding));
    let binding_after = fs::symlink_metadata(&binding).unwrap();
    assert_eq!(
        (
            binding_after.dev(),
            binding_after.ino(),
            binding_after.len(),
            binding_after.mode(),
            (binding_after.mtime(), binding_after.mtime_nsec()),
            (binding_after.ctime(), binding_after.ctime_nsec()),
        ),
        binding_identity,
        "original Params provider identity changed"
    );
    for (path, digest) in source_before {
        assert_eq!(
            sha(&fs::read(path).unwrap()),
            digest,
            "opendbc values changed during generation"
        );
    }
}
