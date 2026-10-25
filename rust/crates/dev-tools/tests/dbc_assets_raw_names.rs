//! Original copied DBC basenames use Unix surrogateescape, including JSON keys.
use openpilot_dev_tools::{
    dbc_assets,
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::ffi::OsStringExt,
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
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn original() -> Value {
    let raw = include_bytes!("fixtures/dbc-assets/raw-names-original.json");
    assert_eq!(
        digest(raw),
        "18f4d69fca04f882a8b219bd40d018c2236bb2715f7e4a453e7871974919fcfb"
    );
    serde_json::from_slice(raw).unwrap()
}
fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_name() == "__pycache__"
            || entry.path().extension().is_some_and(|v| v == "pyc")
        {
            continue;
        }
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
fn input(directory: &Path, row: &Value) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repository = directory.join("repo");
    let source = repository.join("opendbc_repo/opendbc/dbc");
    copy_tree(&root().join("opendbc_repo/opendbc/dbc"), &source)?;
    fs::write(
        source.join(OsString::from_vec(unhex(
            row["filename_hex"].as_str().unwrap(),
        ))),
        unhex(row["file_bytes_hex"].as_str().unwrap()),
    )?;
    Ok(repository)
}
fn verify(destination: &Path, row: &Value) -> TestResult {
    let consumer = row["consumer"].as_str().unwrap();
    let raw_name = OsString::from_vec(unhex(row["filename_hex"].as_str().unwrap()));
    assert_eq!(
        fs::read(destination.join(&raw_name))?,
        unhex(row["file_bytes_hex"].as_str().unwrap())
    );
    let file = if consumer == "card" {
        "card-assets.json"
    } else {
        "radarcan-assets.json"
    };
    let raw = fs::read(destination.join(file))?;
    let json_key = row["json_key_ascii"].as_str().unwrap().as_bytes();
    assert!(
        raw.windows(json_key.len()).any(|part| part == json_key),
        "original ensure_ascii surrogate key missing"
    );
    let document = JsonValue::parse(std::str::from_utf8(&raw)?)?;
    let assets = document.get("asset_sha256").ok_or("missing assets")?;
    let JsonView::Object(entries) = assets.view() else {
        panic!("asset map")
    };
    assert_eq!(entries.len() as u64, row["asset_count"].as_u64().unwrap());
    let points: Vec<u32> = serde_json::from_value(row["filename_codepoints"].clone())?;
    let (_, checksum) = entries
        .iter()
        .find(|(name, _)| *name == points.as_slice())
        .ok_or("lost raw codepoint key")?;
    assert!(checksum.text_eq(row["asset_sha256"].as_str().unwrap()));
    let source_hashes = document.get("source_sha256").ok_or("missing source pins")?;
    let JsonView::Object(source_entries) = source_hashes.view() else {
        panic!("source hash map")
    };
    let mut source_points: Vec<u32> = if consumer == "card" {
        "opendbc_repo/opendbc/dbc/".chars().map(u32::from).collect()
    } else {
        Vec::new()
    };
    source_points.extend_from_slice(&points);
    let (_, source_checksum) = source_entries
        .iter()
        .find(|(name, _)| *name == source_points.as_slice())
        .ok_or("lost raw source provenance key")?;
    assert!(source_checksum.text_eq(row["asset_sha256"].as_str().unwrap()));
    assert_eq!(
        digest(&fs::read(destination.join(raw_name))?),
        row["asset_sha256"].as_str().unwrap()
    );
    let catalog_raw = include_bytes!("fixtures/dbc-assets/catalog-original.json");
    assert_eq!(
        digest(catalog_raw),
        "57278d6bf03b3cd7ea0c00238c6082657c37512056adc423079abe80562ea6cc"
    );
    let catalog: Value = serde_json::from_slice(catalog_raw)?;
    for (name, checksum) in catalog[consumer]["manifest"]["asset_sha256"]
        .as_object()
        .unwrap()
    {
        assert!(assets
            .get(name)
            .ok_or("missing normal asset")?
            .text_eq(checksum.as_str().unwrap()));
        assert_eq!(
            digest(&fs::read(destination.join(name))?),
            checksum.as_str().unwrap()
        );
    }
    Ok(())
}
fn api(consumer: &str) -> TestResult {
    let capture = original();
    let row = capture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["consumer"] == consumer)
        .unwrap();
    let directory = tempfile::tempdir()?;
    let repository = input(directory.path(), row)?;
    let destination = directory.path().join("dbc");
    // The original succeeds. A Unicode admission error or lossy key replacement
    // must not be accepted as a successful native packaging equivalence.
    let returned = if consumer == "card" {
        dbc_assets::card(&repository, &destination)?
    } else {
        dbc_assets::radarcan(&repository.join("opendbc_repo/opendbc/dbc"), &destination)?
    };
    let returned_assets = returned
        .get("asset_sha256")
        .ok_or("API omitted asset map")?;
    let JsonView::Object(entries) = returned_assets.view() else {
        panic!("API asset map")
    };
    let points: Vec<u32> = serde_json::from_value(row["filename_codepoints"].clone())?;
    let (_, checksum) = entries
        .iter()
        .find(|(name, _)| *name == points.as_slice())
        .ok_or("API lost raw U+DCFF key")?;
    assert!(checksum.text_eq(row["asset_sha256"].as_str().unwrap()));
    let name = if consumer == "card" {
        "card-assets.json"
    } else {
        "radarcan-assets.json"
    };
    let saved = JsonValue::parse(&fs::read_to_string(destination.join(name))?)?;
    assert_eq!(
        returned.to_json()?,
        saved.to_json()?,
        "full lossless API result differs from published manifest"
    );
    verify(&destination, row)
}
#[test]
fn original_card_raw_copied_basename_preserves_98_assets_and_surrogate_json_key() -> TestResult {
    api("card")
}
#[test]
fn original_radar_raw_copied_basename_preserves_104_assets_and_surrogate_json_key() -> TestResult {
    api("radarcan")
}

#[test]
fn actual_cli_keeps_raw_basename_codepoints_with_no_python_path() -> TestResult {
    let capture = original();
    for row in capture["cases"].as_array().unwrap() {
        let consumer = row["consumer"].as_str().unwrap();
        let directory = tempfile::tempdir()?;
        let repository = input(directory.path(), row)?;
        let destination = directory.path().join("dbc");
        let binary = if consumer == "card" {
            env!("CARGO_BIN_EXE_prepare-card-assets")
        } else {
            env!("CARGO_BIN_EXE_prepare-radarcan-assets")
        };
        let mut args: Vec<OsString> = vec![binary.into()];
        if consumer == "card" {
            args.push(repository.as_os_str().to_owned());
        }
        args.push(destination.as_os_str().to_owned());
        let mut spec = CommandSpec::new_os(args, Duration::from_secs(30));
        spec.set_os_env(BTreeMap::from([
            ("PATH".into(), "/nonexistent".into()),
            ("OPENPILOT_RUST_ROOT".into(), repository.into_os_string()),
        ]));
        let result = NativeRunner.run(&spec)?;
        assert_eq!(
            result.code,
            0,
            "{consumer}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
        let expected = if consumer == "card" {
            b"Prepared 98 DBC assets with native Rust generator\n".as_slice()
        } else {
            b"Prepared 104 original RadarCAN DBCs with native Rust generator\n".as_slice()
        };
        assert_eq!(result.stdout, expected);
        verify(&destination, row)?;
    }
    Ok(())
}
