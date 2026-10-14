use openpilot_dev_tools::brotli_stage;
use std::{fs, os::unix::fs::symlink, path::Path, process::Command, sync::Arc};

const LIBRARIES: [&str; 3] = [
    "libbrotlienc.so.1",
    "libbrotlidec.so.1",
    "libbrotlicommon.so.1",
];

fn fixture(source: &Path, machine: u16) {
    fs::create_dir_all(source).unwrap();
    for (index, name) in LIBRARIES.into_iter().enumerate() {
        let mut bytes = vec![0; 64 + index];
        bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
        bytes[16..18].copy_from_slice(&3_u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&machine.to_le_bytes());
        *bytes.last_mut().unwrap() = u8::try_from(index).unwrap();
        fs::write(source.join(name), bytes).unwrap();
    }
}

fn clean_parent(parent: &Path) {
    assert!(!fs::read_dir(parent).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".brotli-")));
}

#[test]
fn both_targets_match_unchanged_original_manifest_bytes_and_report() {
    for (target, machine, expected) in [
        (
            "x86_64-unknown-linux-gnu",
            62,
            include_str!("fixtures/brotli/x86_64-manifest.json"),
        ),
        (
            "aarch64-unknown-linux-gnu",
            183,
            include_str!("fixtures/brotli/aarch64-manifest.json"),
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fixture(&source, machine);
        let output = temp.path().join("bundle");
        let report = brotli_stage::stage(&source, &output, target, "1.1.0").unwrap();
        assert_eq!(
            fs::read_to_string(output.join("manifest.json")).unwrap(),
            expected
        );
        for name in LIBRARIES {
            assert_eq!(
                fs::read(source.join(name)).unwrap(),
                fs::read(output.join(name)).unwrap()
            );
            assert!(!output.join(name).is_symlink());
        }
        let report = serde_json::to_value(report).unwrap();
        assert_eq!(
            report["manifest"],
            serde_json::from_str::<serde_json::Value>(expected).unwrap()
        );
        assert_eq!(report["bytes"], 195);
        assert_eq!(
            report["bundle"],
            output.canonicalize().unwrap().to_str().unwrap()
        );
        clean_parent(temp.path());
    }
}

#[test]
fn internal_sdk_symlinks_are_followed_but_external_sources_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fixture(&source, 62);
    fs::rename(source.join(LIBRARIES[0]), source.join("encoder.versioned")).unwrap();
    symlink("encoder.versioned", source.join(LIBRARIES[0])).unwrap();
    brotli_stage::stage(
        &source,
        &temp.path().join("good"),
        "x86_64-unknown-linux-gnu",
        "1.1.0",
    )
    .unwrap();
    fs::remove_file(source.join(LIBRARIES[0])).unwrap();
    symlink(
        temp.path().join("good").join(LIBRARIES[0]),
        source.join(LIBRARIES[0]),
    )
    .unwrap();
    assert!(brotli_stage::stage(
        &source,
        &temp.path().join("bad"),
        "x86_64-unknown-linux-gnu",
        "1.1.0"
    )
    .is_err());
    assert!(!temp.path().join("bad").exists());
    clean_parent(temp.path());
}

#[test]
fn invalid_target_version_and_elf_never_publish() {
    for (target, version) in [
        ("bad", "1.1.0"),
        ("x86_64-unknown-linux-gnu", "2.0.0"),
        ("x86_64-unknown-linux-gnu", "1.4096.0"),
        ("x86_64-unknown-linux-gnu", "1.0.4096"),
        ("x86_64-unknown-linux-gnu", "1.0"),
        ("x86_64-unknown-linux-gnu", "1.-1.0"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        fixture(&temp.path().join("source"), 62);
        let output = temp.path().join("output");
        assert!(
            brotli_stage::stage(&temp.path().join("source"), &output, target, version).is_err()
        );
        assert!(!output.exists());
    }
    for (offset, replacement) in [(0, 0), (4, 1), (5, 2), (16, 2), (18, 183)] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fixture(&source, 62);
        let mut bytes = fs::read(source.join(LIBRARIES[2])).unwrap();
        bytes[offset] = replacement;
        fs::write(source.join(LIBRARIES[2]), bytes).unwrap();
        let output = temp.path().join("output");
        assert!(
            brotli_stage::stage(&source, &output, "x86_64-unknown-linux-gnu", "1.1.0").is_err()
        );
        assert!(!output.exists());
        clean_parent(temp.path());
    }
}

#[test]
fn oversized_sparse_input_is_rejected_without_publication() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fixture(&source, 62);
    fs::OpenOptions::new()
        .write(true)
        .open(source.join(LIBRARIES[2]))
        .unwrap()
        .set_len(1 << 40)
        .unwrap();
    let output = temp.path().join("output");
    let error =
        brotli_stage::stage(&source, &output, "x86_64-unknown-linux-gnu", "1.1.0").unwrap_err();
    assert!(error.to_string().contains("size"));
    assert!(!output.exists());
    clean_parent(temp.path());
}

#[test]
fn existing_destinations_and_parallel_publish_never_clobber() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fixture(&source, 62);
    for kind in 0..3 {
        let output = temp.path().join(format!("existing-{kind}"));
        match kind {
            0 => fs::create_dir(&output).unwrap(),
            1 => fs::write(&output, b"kept").unwrap(),
            _ => symlink("absent", &output).unwrap(),
        }
        assert!(
            brotli_stage::stage(&source, &output, "x86_64-unknown-linux-gnu", "1.1.0").is_err()
        );
        assert!(output.exists() || output.is_symlink());
        if kind == 1 {
            assert_eq!(fs::read(&output).unwrap(), b"kept");
        }
    }
    let source = Arc::new(source);
    let output = Arc::new(temp.path().join("parallel"));
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let children: Vec<_> = (0..2)
        .map(|_| {
            let (source, output, barrier) = (
                Arc::clone(&source),
                Arc::clone(&output),
                Arc::clone(&barrier),
            );
            std::thread::spawn(move || {
                barrier.wait();
                brotli_stage::stage(&source, &output, "x86_64-unknown-linux-gnu", "1.1.0").is_ok()
            })
        })
        .collect();
    assert_eq!(
        children
            .into_iter()
            .map(|child| child.join().unwrap())
            .filter(|passed| *passed)
            .count(),
        1
    );
    assert_eq!(
        fs::read_to_string(output.join("manifest.json")).unwrap(),
        include_str!("fixtures/brotli/x86_64-manifest.json")
    );
    clean_parent(temp.path());
}

#[test]
fn native_cli_stages_without_python_or_path_search() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fixture(&source, 62);
    let output = temp.path().join("output");
    let result = Command::new(env!("CARGO_BIN_EXE_stage-brotli-provider"))
        .arg(&source)
        .arg(&output)
        .args(["x86_64-unknown-linux-gnu", "1.1.0"])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap()["bytes"],
        195
    );
}

#[test]
fn maximum_library_size_is_inclusive_and_oversize_is_rejected() {
    for (size, accepted) in [(16 * 1024 * 1024, true), (16 * 1024 * 1024 + 1, false)] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fixture(&source, 62);
        fs::OpenOptions::new()
            .write(true)
            .open(source.join(LIBRARIES[0]))
            .unwrap()
            .set_len(size)
            .unwrap();
        let output = temp.path().join("output");
        let result = brotli_stage::stage(&source, &output, "x86_64-unknown-linux-gnu", "1.1.0");
        assert_eq!(result.is_ok(), accepted, "size={size}");
        assert_eq!(output.exists(), accepted);
        if accepted {
            assert_eq!(fs::metadata(output.join(LIBRARIES[0])).unwrap().len(), size);
        }
        clean_parent(temp.path());
    }
}

#[test]
fn missing_and_nonregular_inputs_never_publish() {
    for directory in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fixture(&source, 62);
        fs::remove_file(source.join(LIBRARIES[1])).unwrap();
        if directory {
            fs::create_dir(source.join(LIBRARIES[1])).unwrap();
        }
        let output = temp.path().join("output");
        assert!(
            brotli_stage::stage(&source, &output, "x86_64-unknown-linux-gnu", "1.1.0").is_err()
        );
        assert!(!output.exists());
        clean_parent(temp.path());
    }
}

#[test]
fn packed_version_boundaries_match_original_contract() {
    for (version, packed) in [("1.0.0", 1_u32 << 24), ("1.4095.4095", (1_u32 << 25) - 1)] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fixture(&source, 62);
        let output = temp.path().join("output");
        let report =
            brotli_stage::stage(&source, &output, "x86_64-unknown-linux-gnu", version).unwrap();
        assert_eq!(
            serde_json::to_value(report).unwrap()["manifest"]["brotli_version"],
            packed
        );
    }
}
