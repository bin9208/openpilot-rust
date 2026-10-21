//! Original-only CLI admissions captured in Actions; the original checker is
//! still a live dependency of the driving-daemon oracle.
#![cfg(all(target_os = "linux", feature = "desire-reference-native"))]

use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
    },
    path::{Path, PathBuf},
    time::Duration,
};

const CAPTURE: &[u8] = include_bytes!("fixtures/desire-original-6e34f78a/cli-original.json");
const CAPTURE_SHA: &str = "ac3e8ec4a5117054c073c8761d454ef60a4b599ab16beb9efd647b78364711cf";
const HISTORICAL: &str = "rust/tools/check_desire_reference.py";
const HISTORICAL_SHA: &str = "b5529bcac8036ec559de785319b8bd131963fe05eb9aa149528992f7b4f88f8e";

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn required(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("mandatory Desire CLI proof: {name}")),
    );
    assert!(path.is_absolute(), "{name} must be absolute");
    path
}
fn regular(path: &Path) -> File {
    let file = File::from(
        rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .unwrap(),
    );
    assert!(
        file.metadata().unwrap().is_file(),
        "nonregular: {}",
        path.display()
    );
    file
}
fn identity(file: &File) -> (u64, u64, u64, i64, i64, i64, i64) {
    let metadata = file.metadata().unwrap();
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}
fn opened_sha(file: &mut File, maximum: u64) -> String {
    assert!(file.metadata().unwrap().len() <= maximum);
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut hash = Sha256::new();
    let mut chunk = [0_u8; 65536];
    let mut total = 0_u64;
    loop {
        let room = (maximum.saturating_sub(total) + 1).min(chunk.len() as u64) as usize;
        let count = file.read(&mut chunk[..room]).unwrap();
        if count == 0 {
            break;
        }
        total += count as u64;
        assert!(total <= maximum);
        hash.update(&chunk[..count]);
    }
    format!("{:x}", hash.finalize())
}
struct HistoricalSource {
    path: PathBuf,
    held: File,
    identity: (u64, u64, u64, i64, i64, i64, i64),
}
impl HistoricalSource {
    fn open(path: PathBuf) -> Self {
        let mut held = regular(&path);
        let seal = identity(&held);
        assert_eq!(opened_sha(&mut held, 1_000_000), HISTORICAL_SHA);
        assert_eq!(identity(&held), seal);
        let source = Self {
            path,
            held,
            identity: seal,
        };
        source.verify();
        source
    }
    fn verify(&self) {
        let mut current = regular(&self.path);
        assert_eq!(identity(&current), self.identity);
        assert_eq!(identity(&self.held), self.identity);
        assert_eq!(opened_sha(&mut current, 1_000_000), HISTORICAL_SHA);
        assert_eq!(identity(&current), self.identity);
        assert_eq!(identity(&self.held), self.identity);
        assert_eq!(
            self.path.symlink_metadata().unwrap().ino(),
            self.identity.1,
            "original Desire checker path changed"
        );
    }
}
fn file_sha(path: &Path, maximum: u64) -> String {
    let mut file = regular(path);
    assert!(
        file.metadata().unwrap().len() <= maximum,
        "oversize: {}",
        path.display()
    );
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut hash = Sha256::new();
    let mut chunk = [0_u8; 65536];
    let mut total = 0_u64;
    loop {
        let room = (maximum.saturating_sub(total) + 1).min(chunk.len() as u64) as usize;
        let count = file.read(&mut chunk[..room]).unwrap();
        if count == 0 {
            break;
        }
        total += count as u64;
        assert!(total <= maximum, "oversize: {}", path.display());
        hash.update(&chunk[..count]);
    }
    format!("{:x}", hash.finalize())
}
fn read_bounded(path: &Path, maximum: u64) -> Vec<u8> {
    let mut file = regular(path).take(maximum + 1);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert!(
        bytes.len() as u64 <= maximum,
        "oversize: {}",
        path.display()
    );
    bytes
}
fn hex_bytes(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn substitute(mut token: Vec<u8>, marker: &[u8], replacement: &[u8]) -> Vec<u8> {
    if let Some(at) = token
        .windows(marker.len())
        .position(|window| window == marker)
    {
        token.splice(at..at + marker.len(), replacement.iter().copied());
    }
    token
}
fn source_elf(path: &Path) -> (u64, u64, u64, String) {
    let mut file = regular(path);
    let metadata = file.metadata().unwrap();
    let mut header = [0_u8; 64];
    file.read_exact(&mut header).unwrap();
    let machine = match std::env::consts::ARCH {
        "x86_64" => 62,
        "aarch64" => 183,
        other => panic!("unsupported Desire CLI host: {other}"),
    };
    assert_eq!(&header[..4], b"\x7fELF");
    assert_eq!((header[4], header[5], header[6]), (2, 1, 1));
    assert!(matches!(
        u16::from_le_bytes([header[16], header[17]]),
        2 | 3
    ));
    assert_eq!(u16::from_le_bytes([header[18], header[19]]), machine);
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        file_sha(path, 100_000_000),
    )
}

#[test]
fn original_cli_channels_and_full_raw_path_match_native_with_pinned_source() {
    assert_eq!(sha(CAPTURE), CAPTURE_SHA);
    let capture: Value = serde_json::from_slice(CAPTURE).unwrap();
    assert_eq!(capture["kind"], "independently captured unchanged original DesireHelper CLI observations; help/error text retained as evidence, admission tests compare statuses and channels");
    assert_eq!(
        capture["source_commit"],
        "1de0b0d7fbe86e6855d58c8fa9dbb038b5d44126"
    );
    assert_eq!(capture["artifact"], 12323365248_u64);
    assert_eq!(
        capture["artifact_zip_sha256"],
        "e0abcb079d7cf91e0fc52cd7e1318bff106662ef3fd5ff6ff91b33dc1bbe06cc"
    );
    assert_eq!(
        capture["original_source_sha256"],
        "b5529bcac8036ec559de785319b8bd131963fe05eb9aa149528992f7b4f88f8e"
    );
    let cases = capture["cases"].as_array().unwrap();
    let roster = [
        "help_long",
        "help_short",
        "help_cluster",
        "help_after_unknown",
        "unknown",
        "unknown_after_complete",
        "missing_both",
        "missing_binary",
        "missing_output",
        "missing_binary_value",
        "missing_output_value",
        "abbreviated_binary",
        "abbreviated_output",
        "repeated_binary",
        "repeated_output",
        "equals_binary_missing_output",
        "double_dash_before_help",
        "non_utf8_option_before_help",
        "non_utf8_value_before_help",
        "valid_raw_output_path",
    ];
    assert_eq!(cases.len(), roster.len());
    let repository =
        fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")).unwrap();
    let historical = HistoricalSource::open(repository.join(HISTORICAL));
    let checker = fs::canonicalize(required("DESIRE_REFERENCE_CHECKER")).unwrap();
    let probe = fs::canonicalize(required("DESIRE_REFERENCE_PROBE")).unwrap();
    assert_ne!(checker, probe);
    let checker_seal = source_elf(&checker);
    let probe_seal = source_elf(&probe);
    let output = required("DESIRE_REFERENCE_CLI_OUTPUT");
    fs::create_dir_all(&output).unwrap();
    let proof = tempfile::Builder::new()
        .prefix("desire-cli-")
        .tempdir_in(output)
        .unwrap()
        .keep();

    for (row, name) in cases.iter().zip(roster) {
        assert_eq!(row["case"], name);
        historical.verify();
        assert_eq!(source_elf(&checker), checker_seal);
        assert_eq!(source_elf(&probe), probe_seal);
        let side = proof.join(name);
        fs::create_dir(&side).unwrap();
        let generated = side.join(OsString::from_vec(hex_bytes(
            row["output_basename_hex"].as_str().unwrap(),
        )));
        let other = side.join("unused-second-output");
        let mut argv = vec![checker.as_os_str().to_owned()];
        for token in row["tokens_hex"].as_array().unwrap() {
            let token = hex_bytes(token.as_str().unwrap());
            let token = substitute(token, b"@PROBE@", probe.as_os_str().as_bytes());
            let token = substitute(token, b"@OUTPUT@", generated.as_os_str().as_bytes());
            let token = substitute(token, b"@OTHER_OUTPUT@", other.as_os_str().as_bytes());
            argv.push(OsString::from_vec(token));
        }
        if name == "valid_raw_output_path" {
            assert!(generated.as_os_str().as_bytes().contains(&0xff));
        }
        let mut spec = CommandSpec::new_os(
            argv,
            Duration::from_secs(if name == "valid_raw_output_path" {
                360
            } else {
                30
            }),
        );
        spec.set_os_env(BTreeMap::from([
            (
                OsString::from("OPENPILOT_RUST_ROOT"),
                repository.as_os_str().to_owned(),
            ),
            (
                OsString::from("PYTHONDONTWRITEBYTECODE"),
                OsString::from("1"),
            ),
        ]));
        let stdout = side.join("stdout.bin");
        let stderr = side.join("stderr.bin");
        let result = NativeRunner.run_to_files(&spec, &stdout, &stderr).unwrap();
        let out = read_bounded(&stdout, 16_000_000);
        let err = read_bounded(&stderr, 16_000_000);
        let original_out = hex_bytes(row["stdout_hex"].as_str().unwrap());
        let original_err = hex_bytes(row["stderr_hex"].as_str().unwrap());
        assert_eq!(
            result.code,
            row["exit_code"].as_i64().unwrap() as i32,
            "{name}"
        );
        assert_eq!(
            out.is_empty(),
            original_out.is_empty(),
            "stdout channel: {name}"
        );
        assert_eq!(
            err.is_empty(),
            original_err.is_empty(),
            "stderr channel: {name}"
        );
        if name == "valid_raw_output_path" {
            assert_eq!(result.code, 0);
            let original_report: Value = serde_json::from_slice(&original_out).unwrap();
            assert_eq!(
                original_report, row["report"],
                "sealed original stdout report"
            );
            let report: Value = serde_json::from_slice(&out).unwrap();
            assert_eq!(report, original_report, "original stdout report");
            let written_report: Value =
                serde_json::from_slice(&read_bounded(&generated.join("report.json"), 1_000_000))
                    .unwrap();
            assert_eq!(written_report, row["report"], "original written report");
            assert_eq!(report["frames"], 24_585);
            let files = row["files"].as_object().unwrap();
            assert_eq!(files.len(), 3);
            for filename in ["request.jsonl", "reference.jsonl", "actual.jsonl"] {
                let expected = &files[filename];
                let path = generated.join(filename);
                assert_eq!(
                    regular(&path).metadata().unwrap().len(),
                    expected["bytes"].as_u64().unwrap()
                );
                assert_eq!(
                    file_sha(&path, 150_000_000),
                    expected["sha256"].as_str().unwrap(),
                    "{filename}: accepted raw output bytes"
                );
            }
        } else {
            assert!(
                matches!(fs::symlink_metadata(&generated), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
                "unexpected generated output: {name}"
            );
        }
        assert!(
            matches!(fs::symlink_metadata(&other), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
            "unexpected other output: {name}"
        );
        historical.verify();
        assert_eq!(source_elf(&checker), checker_seal);
        assert_eq!(source_elf(&probe), probe_seal);
    }
}
