//! Independent original card/RadarCAN packaging contracts; native API RED first.
use openpilot_dev_tools::dbc_assets::{self, card, radarcan};
use openpilot_log_format::JsonValue;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

type TestResult = Result<(), Box<dyn std::error::Error>>;
// These original catalogs contain ASCII keys only. Raw filesystem keys are
// checked losslessly in dbc_assets_raw_names, never through this test adapter.
fn ordinary(value: JsonValue) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&value.to_json()?)?)
}
fn error(result: Result<JsonValue, dbc_assets::Error>) -> dbc_assets::Error {
    match result {
        Err(error) => error,
        Ok(_) => panic!("original refusal must remain a failure"),
    }
}
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fixture(name: &str) -> Value {
    let (bytes, expected): (&[u8], &str) = match name {
        "catalog" => (
            include_bytes!("fixtures/dbc-assets/catalog-original.json"),
            "57278d6bf03b3cd7ea0c00238c6082657c37512056adc423079abe80562ea6cc",
        ),
        "failures" => (
            include_bytes!("fixtures/dbc-assets/failures-original.json"),
            "45f27f1898a988e64244bcb328cc84903006856d4ba61c198a96d2dc7bdf11c6",
        ),
        "provenance" => (
            include_bytes!("fixtures/dbc-assets/provenance.json"),
            "3f05577508f4b08299133c0e0850ba21dd00876167feecc217c98125f7de4923",
        ),
        "paths" => (
            include_bytes!("fixtures/dbc-assets/paths-original.json"),
            "51fd91423000aa281e03a2e95636b596671cfc6916e74a7270eda44ea31527d3",
        ),
        "boundaries" => (
            include_bytes!("fixtures/dbc-assets/boundaries-original.json"),
            "21b0fb62dab795c58c1157834c3f0d831a02642261510e043cb51affcfeab8f1",
        ),
        "raw-components" => (
            include_bytes!("fixtures/dbc-assets/raw-components-original.json"),
            "06a74814c31f0711ef5189c91a167dc33911de38b465a1d78c5d30e9f63859f7",
        ),
        _ => panic!("unknown immutable original fixture"),
    };
    assert_eq!(hash(bytes), expected);
    serde_json::from_slice(bytes).unwrap()
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
fn source_tree(path: &Path) -> std::io::Result<BTreeMap<String, String>> {
    fn visit(
        base: &Path,
        directory: &Path,
        out: &mut BTreeMap<String, String>,
    ) -> std::io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_name() == "__pycache__" || path.extension().is_some_and(|v| v == "pyc") {
                continue;
            }
            if entry.file_type()?.is_dir() {
                visit(base, &path, out)?;
            } else {
                out.insert(
                    path.strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    hash(&fs::read(path)?),
                );
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    visit(path, path, &mut out)?;
    Ok(out)
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
fn expected_assets(consumer: &str) -> BTreeMap<String, String> {
    serde_json::from_value(fixture("catalog")[consumer]["manifest"]["asset_sha256"].clone())
        .unwrap()
}
fn provenance(manifest: &Value, original: &Value) -> TestResult {
    let native = &manifest["generator_provenance"];
    assert_eq!(native["implementation"], "native");
    let sources = native["source_sha256"]
        .as_object()
        .ok_or("missing native generator source identities")?;
    assert!(sources.contains_key("rust/crates/dev-tools/src/dbc_generator.rs"));
    assert!(sources.contains_key("rust/crates/dev-tools/src/dbc_assets.rs"));
    for (name, digest) in sources {
        assert_eq!(
            hash(&fs::read(root().join(name))?),
            digest.as_str().unwrap(),
            "{name}"
        );
    }
    assert_eq!(
        native["executable_sha256"],
        hash(&fs::read(std::env::current_exe()?)?)
    );
    // Historical Python pins are explicitly labeled, never claimed as current
    // executable inputs after deletion. Current native source/ELF pins are separate.
    let historical = fixture("provenance");
    for (name, digest) in historical["reference_python_sha256"].as_object().unwrap() {
        assert_eq!(
            native["reference_python_sha256"][name], *digest,
            "lost historical pin {name}"
        );
    }
    for (name, digest) in original["source_sha256"].as_object().unwrap() {
        let path = if name.starts_with("opendbc_repo/") {
            root().join(name)
        } else {
            root().join("opendbc_repo/opendbc/dbc").join(name)
        };
        if name == "SConscript" {
            // This caller is intentionally migrated after original capture.
            // Its historical byte identity stays in the immutable archive pin;
            // the live asset receipt must describe the current actual input.
            assert_eq!(manifest["source_sha256"][name], hash(&fs::read(path)?));
        } else if name.ends_with(".py") && !path.exists() {
            let key = name
                .strip_prefix("opendbc_repo/opendbc/dbc/")
                .unwrap_or(name);
            assert_eq!(native["reference_python_sha256"][key], *digest);
        } else {
            assert_eq!(
                manifest["source_sha256"][name], *digest,
                "lost actual input pin {name}"
            );
        }
    }
    Ok(())
}

#[test]
fn card_original_97_assets_subset_and_source_isolation_are_preserved() -> TestResult {
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("dbc");
    let source = root().join("opendbc_repo/opendbc/dbc");
    let before = source_tree(&source)?;
    let manifest = ordinary(card(root(), &destination)?)?;
    assert_eq!(assets(&destination)?, expected_assets("card"));
    assert_eq!(assets(&destination)?.len(), 97);
    assert_eq!(source_tree(&source)?, before);
    assert_eq!(manifest["format_version"], 1);
    assert_eq!(manifest["build_time_only"], true);
    let saved: Value = serde_json::from_slice(&fs::read(destination.join("card-assets.json"))?)?;
    assert_eq!(saved, manifest);
    provenance(&manifest, &fixture("catalog")["card"]["manifest"])?;
    for entry in fs::read_dir(&destination)? {
        assert_ne!(
            entry?.path().extension().and_then(|v| v.to_str()),
            Some("py")
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        for entry in fs::read_dir(&source)? {
            let entry = entry?;
            let name = entry.file_name();
            if entry.path().extension().is_some_and(|v| v == "dbc")
                && destination.join(&name).exists()
            {
                assert_ne!(
                    fs::metadata(entry.path())?.ino(),
                    fs::metadata(destination.join(name))?.ino()
                );
            }
        }
    }
    Ok(())
}

#[test]
fn radar_original_103_assets_and_live_space_input_metadata_are_preserved() -> TestResult {
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("dbc");
    let source = root().join("opendbc_repo/opendbc/dbc");
    let before = source_tree(&source)?;
    let input_size: u64 = before
        .keys()
        .map(|name| fs::metadata(source.join(name)).unwrap().len())
        .sum();
    let report = ordinary(radarcan(&source, &destination)?)?;
    assert_eq!(assets(&destination)?, expected_assets("radarcan"));
    assert_eq!(assets(&destination)?.len(), 103);
    assert_eq!(source_tree(&source)?, before);
    assert_eq!(report["build_time_only"], true);
    assert_eq!(
        report["growth_bound"].as_u64(),
        Some(4 * input_size + 64 * 1024 * 1024)
    );
    assert!(
        report["free_before"].as_u64().unwrap()
            >= 25 * 1024u64.pow(3) + report["growth_bound"].as_u64().unwrap()
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(destination.join("radarcan-assets.json"))?)?,
        report
    );
    provenance(&report, &fixture("catalog")["radarcan"]["manifest"])?;
    Ok(())
}

#[test]
fn original_existing_destinations_and_missing_parent_errors_preserve_evidence() -> TestResult {
    for row in fixture("failures")["api"].as_array().unwrap() {
        let mode = row["mode"].as_str().unwrap();
        if !["existing-dir", "existing-file", "missing-parent"].contains(&mode) {
            continue;
        }
        let directory = tempfile::tempdir()?;
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
        let result: Result<JsonValue, dbc_assets::Error> = if row["consumer"] == "card" {
            card(root(), &destination)
        } else {
            radarcan(&root().join("opendbc_repo/opendbc/dbc"), &destination)
        };
        let error = error(result);
        assert_eq!(error.kind(), row["class"].as_str().unwrap());
        assert_eq!(error.raw_os_error().map(i64::from), row["errno"].as_i64());
        assert_eq!(
            destination.exists(),
            row["destination_exists"].as_bool().unwrap()
        );
        if mode == "existing-dir" {
            assert_eq!(fs::read(destination.join("retained"))?, b"evidence");
        }
        if mode == "existing-file" {
            assert_eq!(fs::read(&destination)?, b"evidence");
        }
    }
    Ok(())
}

#[test]
fn actual_generation_failure_keeps_card_atomic_and_radar_partial_destination() -> TestResult {
    for consumer in ["card", "radarcan"] {
        let directory = tempfile::tempdir()?;
        let repository = directory.path().join("repo");
        let source = repository.join("opendbc_repo/opendbc/dbc");
        copy_tree(&root().join("opendbc_repo/opendbc/dbc"), &source)?;
        let broken = source.join("generator/nissan/nissan_x_trail_2017.dbc");
        let mut bytes = fs::read(&broken)?;
        bytes.extend_from_slice(b"\nCM_ \"IMPORT _consumer_missing.dbc\";\n");
        fs::write(broken, bytes)?;
        let before = source_tree(&source)?;
        let destination = directory.path().join("dbc");
        let result: Result<JsonValue, dbc_assets::Error> = if consumer == "card" {
            card(&repository, &destination)
        } else {
            radarcan(&source, &destination)
        };
        let error = error(result);
        let original = fixture("failures");
        let row = original["api"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["consumer"] == consumer && r["mode"] == "generation-failure")
            .unwrap();
        assert_eq!(error.kind(), row["class"].as_str().unwrap());
        assert_eq!(error.raw_os_error().map(i64::from), row["errno"].as_i64());
        assert_eq!(
            destination.exists(),
            row["destination_exists"].as_bool().unwrap()
        );
        if destination.exists() {
            assert!(fs::read_dir(&destination)?.next().is_none());
        }
        assert_eq!(source_tree(&source)?, before);
        assert!(!fs::read_dir(directory.path())?.any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("card-assets-")));
    }
    Ok(())
}

#[test]
fn original_missing_card_generator_never_publishes_a_package() -> TestResult {
    let directory = tempfile::tempdir()?;
    let repository = directory.path().join("empty-repo");
    fs::create_dir(&repository)?;
    let destination = directory.path().join("dbc");
    let result: Result<JsonValue, dbc_assets::Error> = card(&repository, &destination);
    assert_eq!(error(result).kind(), "FileNotFoundError");
    assert!(!destination.exists());
    Ok(())
}

#[test]
fn immutable_original_consumers_and_catalog_distinctions_remain_explicit() -> TestResult {
    let pins = fixture("provenance");
    for (name, digest) in pins["source_sha256"].as_object().unwrap() {
        let path = root().join(name);
        // A dangling symlink is a present, unreadable source, not retirement.
        match fs::symlink_metadata(&path) {
            Ok(_) => assert_eq!(hash(&fs::read(&path)?), digest.as_str().unwrap(), "{name}"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => assert!([
                "rust/tools/card_prepare_assets.py",
                "rust/tools/radarcan_prepare_assets.py",
                "rust/tools/tests/test_card_assets.py"
            ]
            .contains(&name.as_str())),
            Err(error) => return Err(error.into()),
        }
    }
    // The old lazy facade/SCons pins are archived expectations, not a demand
    // that intentionally migrated current callers retain their old source bytes.
    assert_eq!(pins["archived_caller_sha256"].as_object().unwrap().len(), 3);
    let card = expected_assets("card");
    let radar = expected_assets("radarcan");
    assert_eq!(
        radar.keys().filter(|key| !card.contains_key(*key)).count(),
        6
    );
    let different = card
        .iter()
        .filter(|(key, hash)| radar.get(*key) != Some(*hash))
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        different,
        [
            "hyundai_canfd_corner_radar_180_generated.dbc",
            "hyundai_canfd_corner_radar_235_generated.dbc"
        ]
    );
    let failures = fixture("failures");
    assert_eq!(failures["api"].as_array().unwrap().len(), 9);
    assert_eq!(failures["cli"].as_array().unwrap().len(), 14);
    Ok(())
}

#[cfg(unix)]
#[test]
fn original_dangling_symlinks_and_raw_source_destination_paths_are_preserved() -> TestResult {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };
    let controls = fixture("paths");
    assert_eq!(controls["cases"].as_array().unwrap().len(), 8);
    for row in controls["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir()?;
        let mut repository = root().to_owned();
        let mut destination = directory.path().join("dbc");
        let mode = row["mode"].as_str().unwrap();
        if mode == "dangling-destination" {
            symlink("owned-missing-target", &destination)?;
        }
        if mode == "symlink-directory" {
            fs::create_dir(directory.path().join("retained-target"))?;
            fs::write(directory.path().join("retained-target/marker"), b"evidence")?;
            symlink("retained-target", &destination)?;
        }
        if mode == "raw-destination" {
            destination = directory
                .path()
                .join(OsString::from_vec(b"dbc-\xff".to_vec()));
        }
        if mode == "raw-repository" {
            repository = directory
                .path()
                .join(OsString::from_vec(b"repo-\xfe".to_vec()));
            copy_tree(
                &root().join("opendbc_repo/opendbc/dbc"),
                &repository.join("opendbc_repo/opendbc/dbc"),
            )?;
        }
        let consumer = row["consumer"].as_str().unwrap();
        let result: Result<JsonValue, dbc_assets::Error> = if consumer == "card" {
            card(&repository, &destination)
        } else {
            radarcan(&repository.join("opendbc_repo/opendbc/dbc"), &destination)
        };
        if row["outcome"]["success"].as_bool().unwrap() {
            result?;
            assert_eq!(
                assets(&destination)?,
                expected_assets(consumer),
                "{consumer}/{mode}"
            );
        } else {
            let error = error(result);
            assert_eq!(
                error.kind(),
                row["outcome"]["class"].as_str().unwrap(),
                "{consumer}/{mode}"
            );
            assert_eq!(
                error.raw_os_error().map(i64::from),
                row["outcome"]["errno"].as_i64(),
                "{consumer}/{mode}"
            );
        }
        assert_eq!(
            destination.exists(),
            row["destination_exists"].as_bool().unwrap()
        );
        assert_eq!(
            destination.is_symlink(),
            row["destination_is_symlink"].as_bool().unwrap()
        );
        if let Some(target) = row["symlink_target"].as_str() {
            assert_eq!(fs::read_link(&destination)?, Path::new(target));
        }
        if row["retained_marker"].is_string() {
            assert_eq!(
                fs::read(directory.path().join("retained-target/marker"))?,
                b"evidence"
            );
        }
        assert!(!fs::read_dir(directory.path())?.any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("card-assets-")));
    }
    Ok(())
}

fn boundary_unhex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn original_recursive_manifest_path_component_order_is_preserved() -> TestResult {
    use openpilot_log_format::JsonView;
    let original = fixture("boundaries");
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source");
    copy_tree(&root().join("opendbc_repo/opendbc/dbc"), &source)?;
    for pair in original["files"].as_array().unwrap() {
        let path = source.join(pair[0].as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, boundary_unhex(pair[1].as_str().unwrap()))?;
    }
    let manifest = radarcan(&source, &directory.path().join("dbc"))?;
    let hashes = manifest.get("source_sha256").ok_or("source map")?;
    let JsonView::Object(entries) = hashes.view() else {
        panic!("source map object");
    };
    let actual: Vec<_> = entries
        .iter()
        .map(|(key, _)| {
            key.iter()
                .copied()
                .map(char::from_u32)
                .collect::<Option<String>>()
                .unwrap()
        })
        .collect();
    let historical = fixture("provenance");
    let expected: Vec<_> = original["ordering"]["source_order"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| {
            let name = value.as_str().unwrap();
            match fs::symlink_metadata(source.join(name)) {
                Ok(_) => Some(name.to_owned()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    assert!(name.starts_with("generator/") && name.ends_with(".py"));
                    assert!(
                        historical["reference_python_sha256"].get(name).is_some(),
                        "only the approved retired original roster may be absent: {name}"
                    );
                    None
                }
                Err(error) => panic!("{name}: {error}"),
            }
        })
        .collect();
    assert_eq!(
        actual, expected,
        "Path ordering compares components before later filename suffixes"
    );
    assert_eq!(
        ordinary(manifest)?["asset_sha256"],
        original["ordering"]["asset_sha256"]
    );
    Ok(())
}

#[test]
fn original_component_order_uses_surrogateescape_unicode_not_raw_bytes() -> TestResult {
    use openpilot_log_format::JsonView;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let original = fixture("raw-components");
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source");
    copy_tree(&root().join("opendbc_repo/opendbc/dbc"), &source)?;
    for pair in original["files"].as_array().unwrap() {
        let path = source.join(OsString::from_vec(boundary_unhex(
            pair[0].as_str().unwrap(),
        )));
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, boundary_unhex(pair[1].as_str().unwrap()))?;
    }
    let destination = directory.path().join("dbc");
    let manifest = radarcan(&source, &destination)?;
    let hashes = manifest.get("source_sha256").ok_or("source map")?;
    let JsonView::Object(entries) = hashes.view() else {
        panic!("source map object");
    };
    let actual: Vec<Vec<u32>> = entries
        .iter()
        .filter(|(key, _)| key.starts_with(&[98, 45]))
        .map(|(key, _)| key.to_vec())
        .collect();
    let expected: Vec<Vec<u32>> =
        serde_json::from_value(original["source_order_codepoints"].clone())?;
    assert_eq!(actual, expected);
    let assets = manifest.get("asset_sha256").ok_or("asset map")?;
    let JsonView::Object(entries) = assets.view() else {
        panic!("asset map object");
    };
    assert_eq!(
        entries.len() as u64,
        original["asset_count"].as_u64().unwrap()
    );
    for (name, expected) in original["copied_asset_sha256"].as_object().unwrap() {
        let name = OsString::from_vec(boundary_unhex(name));
        let points = openpilot_dev_tools::python_fs::codepoints(&name);
        let (_, actual) = entries
            .iter()
            .find(|(key, _)| *key == points.as_slice())
            .ok_or("missing copied raw asset")?;
        assert!(actual.text_eq(expected.as_str().unwrap()));
        assert_eq!(
            hash(&fs::read(destination.join(name))?),
            expected.as_str().unwrap()
        );
    }
    Ok(())
}
