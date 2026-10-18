//! Actual native asset entrypoints; expected catalogs/effects are original captures.
use openpilot_dev_tools::qa_process::{Captured, CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
}
fn binary(consumer: &str) -> &'static str {
    match consumer {
        "card" => env!("CARGO_BIN_EXE_prepare-card-assets"),
        "radarcan" => env!("CARGO_BIN_EXE_prepare-radarcan-assets"),
        _ => panic!("unknown original consumer"),
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fixture(name: &str) -> Value {
    let (bytes, digest): (&[u8], &str) = match name {
        "catalog" => (
            include_bytes!("fixtures/dbc-assets/catalog-original.json"),
            "57278d6bf03b3cd7ea0c00238c6082657c37512056adc423079abe80562ea6cc",
        ),
        "failures" => (
            include_bytes!("fixtures/dbc-assets/failures-original.json"),
            "45f27f1898a988e64244bcb328cc84903006856d4ba61c198a96d2dc7bdf11c6",
        ),
        "paths" => (
            include_bytes!("fixtures/dbc-assets/paths-original.json"),
            "51fd91423000aa281e03a2e95636b596671cfc6916e74a7270eda44ea31527d3",
        ),
        "boundaries" => (
            include_bytes!("fixtures/dbc-assets/boundaries-original.json"),
            "21b0fb62dab795c58c1157834c3f0d831a02642261510e043cb51affcfeab8f1",
        ),
        _ => panic!("unknown immutable original fixture"),
    };
    assert_eq!(hash(bytes), digest);
    serde_json::from_slice(bytes).unwrap()
}
fn run(
    consumer: &str,
    repository: &Path,
    cwd: &Path,
    args: Vec<OsString>,
) -> Result<Captured, Box<dyn std::error::Error>> {
    let mut argv = vec![binary(consumer).into()];
    argv.extend(args);
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(30));
    spec.cwd = Some(cwd.to_owned());
    spec.set_os_env(BTreeMap::from([
        ("PATH".into(), "/nonexistent".into()),
        (
            "OPENPILOT_RUST_ROOT".into(),
            repository.as_os_str().to_owned(),
        ),
    ]));
    NativeRunner.run(&spec)
}
fn args(consumer: &str, repository: &Path, destination: &Path) -> Vec<OsString> {
    if consumer == "card" {
        vec![
            repository.as_os_str().to_owned(),
            destination.as_os_str().to_owned(),
        ]
    } else {
        vec![destination.as_os_str().to_owned()]
    }
}
fn banner(consumer: &str) -> &'static [u8] {
    match consumer {
        "card" => b"Prepared 97 DBC assets with native Rust generator\n",
        "radarcan" => b"Prepared 103 original RadarCAN DBCs with native Rust generator\n",
        _ => unreachable!(),
    }
}
fn success(consumer: &str, output: &Captured) {
    assert_eq!(
        output.code,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // An explicit truthful banner transition: original card's Python-used label
    // must not falsely describe the native execution that produced these assets.
    assert_eq!(output.stdout, banner(consumer));
}
fn refusal(output: &Captured, class: &str) {
    assert_eq!(output.code, 1);
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(class),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn assets(path: &Path) -> std::io::Result<BTreeMap<String, String>> {
    fs::read_dir(path)?
        .filter_map(|entry| match entry {
            Ok(entry) if entry.path().extension().is_some_and(|v| v == "dbc") => Some(Ok(entry)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .map(|entry| {
            let entry = entry?;
            Ok((
                entry.file_name().to_string_lossy().into_owned(),
                hash(&fs::read(entry.path())?),
            ))
        })
        .collect()
}
fn expected(consumer: &str) -> BTreeMap<String, String> {
    serde_json::from_value(fixture("catalog")[consumer]["manifest"]["asset_sha256"].clone())
        .unwrap()
}
fn verify_package(consumer: &str, destination: &Path) -> TestResult {
    assert_eq!(assets(destination)?, expected(consumer));
    let name = if consumer == "card" {
        "card-assets.json"
    } else {
        "radarcan-assets.json"
    };
    let bytes = fs::read(destination.join(name))?;
    let manifest: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(manifest["generator_provenance"]["implementation"], "native");
    assert_eq!(
        manifest["generator_provenance"]["executable_sha256"],
        hash(&fs::read(binary(consumer))?)
    );
    assert_eq!(
        manifest["asset_sha256"],
        serde_json::to_value(expected(consumer))?
    );
    assert_eq!(manifest["build_time_only"], true);
    let text = String::from_utf8(bytes)?;
    let keys: &[&str] = if consumer == "card" {
        &[
            "format_version",
            "build_time_only",
            "source_sha256",
            "asset_sha256",
        ]
    } else {
        &[
            "build_time_only",
            "free_before",
            "growth_bound",
            "source_sha256",
            "asset_sha256",
        ]
    };
    let mut cursor = 0;
    for key in keys {
        let needle = format!("\n  \"{key}\":");
        let position = text
            .find(&needle)
            .ok_or("missing original manifest top-level field")?;
        assert!(
            position >= cursor,
            "original manifest field order changed: {key}"
        );
        cursor = position;
    }
    assert!(text.ends_with('\n'));
    Ok(())
}
fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name() == "__pycache__" || path.extension().is_some_and(|v| v == "pyc") {
            continue;
        }
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&path, &target)?;
        } else {
            fs::copy(path, target)?;
        }
    }
    Ok(())
}

#[test]
fn actual_empty_path_catalogs_relative_destinations_and_binary_provenance() -> TestResult {
    for consumer in ["card", "radarcan"] {
        let directory = tempfile::tempdir()?;
        let output = run(
            consumer,
            root(),
            directory.path(),
            args(consumer, root(), Path::new("dbc")),
        )?;
        success(consumer, &output);
        verify_package(consumer, &directory.path().join("dbc"))?;
    }
    Ok(())
}

#[test]
fn original_card_exact_arity_and_radar_ignored_extra_arguments_remain_distinct() -> TestResult {
    let directory = tempfile::tempdir()?;
    let card_missing = run("card", root(), directory.path(), Vec::new())?;
    assert_eq!(card_missing.code, 1);
    assert!(card_missing.stdout.is_empty());
    assert_eq!(
        card_missing.stderr,
        b"usage: prepare-card-assets REPOSITORY NEW_DBC_DIRECTORY\n"
    );
    let mut card_extra_args = args("card", root(), Path::new("card-extra"));
    card_extra_args.push("ignored-extra".into());
    let card_extra = run("card", root(), directory.path(), card_extra_args)?;
    assert_eq!(card_extra.code, 1);
    assert_eq!(card_extra.stdout, card_missing.stdout);
    assert_eq!(card_extra.stderr, card_missing.stderr);
    assert!(!directory.path().join("card-extra").exists());
    let radar_missing = run("radarcan", root(), directory.path(), Vec::new())?;
    assert_eq!(radar_missing.code, 1);
    assert!(radar_missing.stdout.is_empty());
    assert_eq!(
        radar_missing.stderr,
        b"usage: prepare-radarcan-assets NEW_DIRECTORY\n"
    );
    let mut radar_extra = args("radarcan", root(), Path::new("radar-extra"));
    radar_extra.extend([
        "ignored-extra".into(),
        "--source".into(),
        "not-a-source".into(),
    ]);
    let output = run("radarcan", root(), directory.path(), radar_extra)?;
    success("radarcan", &output);
    verify_package("radarcan", &directory.path().join("radar-extra"))?;
    assert_eq!(fixture("failures")["cli"].as_array().unwrap().len(), 14);
    Ok(())
}

#[test]
fn original_existing_targets_and_immediate_missing_parent_fail_without_replacement() -> TestResult {
    for row in fixture("failures")["api"].as_array().unwrap() {
        let mode = row["mode"].as_str().unwrap();
        if !["existing-dir", "existing-file", "missing-parent"].contains(&mode) {
            continue;
        }
        let directory = tempfile::tempdir()?;
        let consumer = row["consumer"].as_str().unwrap();
        let destination = if mode == "missing-parent" {
            directory.path().join("missing/dbc")
        } else {
            directory.path().join("dbc")
        };
        if mode == "existing-dir" {
            fs::create_dir(&destination)?;
            fs::write(destination.join("retained"), b"evidence")?;
        }
        if mode == "existing-file" {
            fs::write(&destination, b"evidence")?;
        }
        let output = run(
            consumer,
            root(),
            directory.path(),
            args(consumer, root(), &destination),
        )?;
        refusal(&output, row["class"].as_str().unwrap());
        assert_eq!(
            destination.exists(),
            row["destination_exists"].as_bool().unwrap()
        );
        if mode == "existing-dir" {
            assert_eq!(fs::read(destination.join("retained"))?, b"evidence");
        }
        if mode == "existing-file" {
            assert_eq!(fs::read(destination)?, b"evidence");
        }
    }
    Ok(())
}

#[test]
fn actual_generation_errors_preserve_card_atomicity_and_radar_empty_destination() -> TestResult {
    for consumer in ["card", "radarcan"] {
        let directory = tempfile::tempdir()?;
        let repository = directory.path().join("repo");
        let source = repository.join("opendbc_repo/opendbc/dbc");
        copy_tree(&root().join("opendbc_repo/opendbc/dbc"), &source)?;
        let broken = source.join("generator/nissan/nissan_x_trail_2017.dbc");
        let mut bytes = fs::read(&broken)?;
        bytes.extend_from_slice(b"\nCM_ \"IMPORT _consumer_missing.dbc\";\n");
        fs::write(&broken, &bytes)?;
        let destination = directory.path().join("dbc");
        let output = run(
            consumer,
            &repository,
            directory.path(),
            args(consumer, &repository, &destination),
        )?;
        refusal(
            &output,
            if consumer == "card" {
                "CalledProcessError"
            } else {
                "FileNotFoundError"
            },
        );
        assert_eq!(fs::read(broken)?, bytes);
        assert_eq!(destination.exists(), consumer == "radarcan");
        if destination.exists() {
            assert!(fs::read_dir(destination)?.next().is_none());
        }
        assert!(!fs::read_dir(directory.path())?.any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("card-assets-")));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn original_dangling_symlink_and_existing_directory_symlink_failures_keep_the_link() -> TestResult {
    use std::os::unix::fs::symlink;
    for row in fixture("paths")["cases"].as_array().unwrap() {
        let mode = row["mode"].as_str().unwrap();
        if !["dangling-destination", "symlink-directory"].contains(&mode) {
            continue;
        }
        let directory = tempfile::tempdir()?;
        let consumer = row["consumer"].as_str().unwrap();
        let destination = directory.path().join("dbc");
        if mode == "symlink-directory" {
            fs::create_dir(directory.path().join("retained-target"))?;
            fs::write(directory.path().join("retained-target/marker"), b"evidence")?;
        }
        let target = row["symlink_target"].as_str().unwrap();
        symlink(target, &destination)?;
        let output = run(
            consumer,
            root(),
            directory.path(),
            args(consumer, root(), &destination),
        )?;
        refusal(&output, row["outcome"]["class"].as_str().unwrap());
        assert_eq!(fs::read_link(destination)?, Path::new(target));
        if mode == "symlink-directory" {
            assert_eq!(
                fs::read(directory.path().join("retained-target/marker"))?,
                b"evidence"
            );
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn actual_raw_destination_and_repository_paths_keep_complete_original_catalogs() -> TestResult {
    use std::os::unix::ffi::OsStringExt;
    for row in fixture("paths")["cases"].as_array().unwrap() {
        let mode = row["mode"].as_str().unwrap();
        if !mode.starts_with("raw-") {
            continue;
        }
        let directory = tempfile::tempdir()?;
        let consumer = row["consumer"].as_str().unwrap();
        let mut repository = root().to_owned();
        let destination: PathBuf = if mode == "raw-destination" {
            directory
                .path()
                .join(OsString::from_vec(b"dbc-\xff".to_vec()))
        } else {
            directory.path().join("dbc")
        };
        if mode == "raw-repository" {
            repository = directory
                .path()
                .join(OsString::from_vec(b"repo-\xfe".to_vec()));
            copy_tree(
                &root().join("opendbc_repo/opendbc/dbc"),
                &repository.join("opendbc_repo/opendbc/dbc"),
            )?;
        }
        let output = run(
            consumer,
            &repository,
            directory.path(),
            args(consumer, &repository, &destination),
        )?;
        success(consumer, &output);
        verify_package(consumer, &destination)?;
    }
    Ok(())
}

#[test]
fn original_fifo_and_symlink_to_fifo_fail_before_open_and_keep_partial_evidence() -> TestResult {
    use std::os::unix::fs::{symlink, FileTypeExt};
    let original = fixture("boundaries");
    let unhex = |value: &str| {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
    };
    for row in original["fifo"].as_array().unwrap() {
        let directory = tempfile::tempdir()?;
        let repository = directory.path().join("repository");
        let source = repository.join("opendbc_repo/opendbc/dbc");
        copy_tree(&root().join("opendbc_repo/opendbc/dbc"), &source)?;
        for pair in original["files"].as_array().unwrap() {
            let path = source.join(pair[0].as_str().unwrap());
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, unhex(pair[1].as_str().unwrap()))?;
        }
        let fifo = source.join("fifo.dbc");
        let target = if row["kind"] == "symlink-fifo" {
            source.join("fifo-target")
        } else {
            fifo.clone()
        };
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &target,
            rustix::fs::Mode::from_raw_mode(0o600),
        )?;
        if target != fifo {
            symlink("fifo-target", &fifo)?;
        }
        let destination = directory.path().join("dbc");
        let mut spec = CommandSpec::new_os(
            vec![
                binary("radarcan").into(),
                destination.as_os_str().to_owned(),
            ],
            Duration::from_secs(5),
        );
        spec.cwd = Some(directory.path().into());
        spec.set_os_env(BTreeMap::from([
            ("PATH".into(), "/nonexistent".into()),
            (
                "OPENPILOT_RUST_ROOT".into(),
                repository.as_os_str().to_owned(),
            ),
        ]));
        let output = NativeRunner.run(&spec).unwrap_or_else(|error| {
            panic!(
                "{}: original FIFO refusal must not block: {error}",
                row["kind"]
            )
        });
        refusal(&output, row["error"].as_str().unwrap());
        assert!(destination.is_dir());
        assert!(!destination.join("radarcan-assets.json").exists());
        let expected: BTreeMap<String, String> =
            serde_json::from_value(row["partial_asset_sha256"].clone())?;
        assert_eq!(
            assets(&destination)?,
            expected,
            "exact original already-copied prefix"
        );
        assert!(
            fs::metadata(&fifo)?.file_type().is_fifo(),
            "owned source input unchanged"
        );
    }
    Ok(())
}
