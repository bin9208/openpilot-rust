//! Full original-only corpus through one freshly built checker and probe ELF pair.
//! Explicit DESIRE_REFERENCE_EXPECT_ABSENT=1 also proves post-deletion checkout absence.
#![cfg(all(target_os = "linux", feature = "desire-reference-native"))]

use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File, Metadata},
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};

const HISTORICAL: &str = "rust/tools/check_desire_reference.py";
const HISTORICAL_SHA: &str = "b5529bcac8036ec559de785319b8bd131963fe05eb9aa149528992f7b4f88f8e";
const MANIFEST_SHA: &str = "ccb84511e949ccd9ebe44c2dc259cd29f5839a42ec35478a1698311b985a81cf";
const FIXTURE: &str = "tests/fixtures/desire-original-6e34f78a/manifest.json";

fn required_path(name: &str) -> PathBuf {
    let value = PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("mandatory cloud prerequisite: {name}")),
    );
    assert!(value.is_absolute(), "{name} must be absolute");
    value
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

fn sha_file(path: &Path) -> String {
    let mut hash = Sha256::new();
    let mut file = regular(path);
    let mut chunk = [0_u8; 65536];
    loop {
        let count = file.read(&mut chunk).unwrap();
        if count == 0 {
            break;
        }
        hash.update(&chunk[..count]);
    }
    format!("{:x}", hash.finalize())
}

#[derive(Debug, Eq, PartialEq, serde::Serialize)]
struct Seal {
    device: u64,
    inode: u64,
    bytes: u64,
    modified_seconds: i64,
    modified_nanos: i64,
    changed_seconds: i64,
    changed_nanos: i64,
    sha256: String,
}
fn seal(path: &Path) -> Seal {
    let file = regular(path);
    let meta: Metadata = file.metadata().unwrap();
    let sha256 = sha_file(path);
    let now = regular(path).metadata().unwrap();
    assert_eq!(
        (
            meta.dev(),
            meta.ino(),
            meta.len(),
            meta.mtime(),
            meta.mtime_nsec(),
            meta.ctime(),
            meta.ctime_nsec()
        ),
        (
            now.dev(),
            now.ino(),
            now.len(),
            now.mtime(),
            now.mtime_nsec(),
            now.ctime(),
            now.ctime_nsec()
        ),
        "file changed while sealing: {}",
        path.display()
    );
    Seal {
        device: meta.dev(),
        inode: meta.ino(),
        bytes: meta.len(),
        modified_seconds: meta.mtime(),
        modified_nanos: meta.mtime_nsec(),
        changed_seconds: meta.ctime(),
        changed_nanos: meta.ctime_nsec(),
        sha256,
    }
}
fn native_elf(path: &Path) {
    let mut file = regular(path);
    let mut header = [0_u8; 64];
    file.read_exact(&mut header).unwrap();
    let machine = match std::env::consts::ARCH {
        "x86_64" => 62,
        "aarch64" => 183,
        other => panic!("unsupported native desire host: {other}"),
    };
    assert_eq!(&header[..4], b"\x7fELF");
    assert_eq!((header[4], header[5], header[6]), (2, 1, 1));
    assert!(matches!(
        u16::from_le_bytes([header[16], header[17]]),
        2 | 3
    ));
    assert_eq!(u16::from_le_bytes([header[18], header[19]]), machine);
}
fn absent(path: &Path) {
    assert!(
        matches!(fs::symlink_metadata(path), Err(error) if error.kind() == io::ErrorKind::NotFound),
        "historical checker reappeared: {}",
        path.display()
    );
}
fn verify_checkout_historical(path: &Path, expect_absent: bool, bound: Option<&Seal>) {
    if expect_absent {
        absent(path);
    } else {
        assert_eq!(seal(path), *bound.expect("present historical source seal"));
    }
}
fn same_bytes(left: &Path, right: &Path) {
    let mut left = regular(left);
    let mut right = regular(right);
    assert_eq!(
        left.metadata().unwrap().len(),
        right.metadata().unwrap().len()
    );
    let mut a = [0_u8; 65536];
    let mut b = [0_u8; 65536];
    loop {
        let len = left.read(&mut a).unwrap();
        if len == 0 {
            assert_eq!(right.read(&mut b[..1]).unwrap(), 0);
        } else {
            right.read_exact(&mut b[..len]).unwrap();
        }
        assert_eq!(&a[..len], &b[..len]);
        if len == 0 {
            break;
        }
    }
}
fn audited_exec(trace: &Path) -> Vec<(String, Vec<String>)> {
    assert!(regular(trace).metadata().unwrap().len() <= 1_000_000);
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let invocation =
        regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    let mut observed = Vec::new();
    for line in fs::read_to_string(trace).unwrap().lines() {
        assert!(
            !line.contains("execve resumed") && !line.contains("execveat resumed"),
            "resumed exec: {line}"
        );
        if !opening.is_match(line) {
            continue;
        }
        let captures = invocation
            .captures(line)
            .unwrap_or_else(|| panic!("failed or opaque exec: {line}"));
        observed.push((
            serde_json::from_str::<String>(&captures[1]).unwrap(),
            serde_json::from_str::<Vec<String>>(&captures[2]).unwrap(),
        ));
    }
    observed
}
fn value(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn run_spec(checker: &Path, probe: &Path, output: &Path, root: &Path, trace: &Path) -> CommandSpec {
    let mut spec = CommandSpec::new_os(
        vec![
            "/usr/bin/strace".into(),
            "-f".into(),
            "-qq".into(),
            "-s".into(),
            "65535".into(),
            "-e".into(),
            "trace=execve,execveat".into(),
            "-o".into(),
            trace.as_os_str().into(),
            checker.as_os_str().into(),
            "--binary".into(),
            probe.as_os_str().into(),
            "--output".into(),
            output.as_os_str().into(),
        ],
        Duration::from_secs(360),
    );
    spec.set_os_env(BTreeMap::from([
        (OsString::from("PATH"), OsString::from("/nonexistent")),
        (
            OsString::from("OPENPILOT_RUST_ROOT"),
            root.as_os_str().into(),
        ),
    ]));
    spec.env_remove = std::env::vars_os()
        .map(|(name, _)| name)
        .filter(|name| {
            name.as_os_str() != OsStr::new("PATH")
                && name.as_os_str() != OsStr::new("OPENPILOT_RUST_ROOT")
        })
        .collect();
    spec
}

#[test]
fn original_only_24585_rows_match_same_elf_checkout_and_historical_absent_root() {
    let expect_absent = match std::env::var_os("DESIRE_REFERENCE_EXPECT_ABSENT") {
        None => false,
        Some(value) if value == OsStr::new("1") => true,
        Some(_) => panic!("DESIRE_REFERENCE_EXPECT_ABSENT must be 1 when set"),
    };
    let repository =
        fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    assert_eq!(sha_file(&fixture), MANIFEST_SHA);
    let manifest = value(&fixture);
    assert_eq!(
        manifest["source_commit"],
        "6e34f78ae77734dac46967cc605836b950c99db9"
    );
    assert_eq!(manifest["frames"], 24_585);
    assert_eq!(manifest["scenario_count"], 47);
    let sources = manifest["source"].as_object().unwrap();
    assert_eq!(sources.len(), 28);
    assert_eq!(sources[HISTORICAL]["before_sha256"], HISTORICAL_SHA);
    assert_eq!(sources[HISTORICAL]["after_sha256"], HISTORICAL_SHA);

    let checker = fs::canonicalize(required_path("DESIRE_REFERENCE_CHECKER")).unwrap();
    let probe = fs::canonicalize(required_path("DESIRE_REFERENCE_PROBE")).unwrap();
    assert_ne!(checker, probe);
    native_elf(&checker);
    native_elf(&probe);
    let executable_seals = BTreeMap::from([
        (checker.clone(), seal(&checker)),
        (probe.clone(), seal(&probe)),
    ]);
    let checkout_historical = repository.join(HISTORICAL);
    if expect_absent {
        absent(&checkout_historical);
    } else {
        assert_eq!(sha_file(&checkout_historical), HISTORICAL_SHA);
    }
    let evidence_root = required_path("DESIRE_REFERENCE_RETIREMENT_OUTPUT");
    fs::create_dir_all(&evidence_root).unwrap();
    let proof = tempfile::Builder::new()
        .prefix("desire-retirement-")
        .tempdir_in(evidence_root)
        .unwrap()
        .keep();
    let absent_root = proof.join("absent-root");
    fs::create_dir_all(absent_root.join("rust/tools")).unwrap();
    let mut source_seals = BTreeMap::new();
    let mut copied_seals = BTreeMap::new();
    for (name, expected) in sources {
        assert_eq!(
            expected["before_sha256"], expected["after_sha256"],
            "{name}"
        );
        let original = repository.join(name);
        if name.as_str() == HISTORICAL && expect_absent {
            absent(&original);
            continue;
        }
        let before = seal(&original);
        assert_eq!(
            expected["before_sha256"].as_str(),
            Some(before.sha256.as_str()),
            "{name}"
        );
        source_seals.insert(name.clone(), before);
        if name.as_str() == HISTORICAL {
            continue;
        }
        let copied = absent_root.join(name);
        fs::create_dir_all(copied.parent().unwrap()).unwrap();
        fs::copy(&original, &copied).unwrap();
        let bound = seal(&copied);
        assert_eq!(
            bound.sha256, source_seals[name].sha256,
            "copied source: {name}"
        );
        copied_seals.insert(name.clone(), bound);
    }
    assert_eq!(source_seals.len(), if expect_absent { 27 } else { 28 });
    assert_eq!(copied_seals.len(), 27);
    let absent_historical = absent_root.join(HISTORICAL);
    absent(&absent_historical);
    fs::write(
        proof.join("inputs-before.json"),
        serde_json::to_vec_pretty(&json!({"executables":&executable_seals,"checkout_sources":&source_seals,"absent_sources":&copied_seals,"checkout_historical_absent":expect_absent})).unwrap(),
    )
    .unwrap();

    let mut first_output: Option<PathBuf> = None;
    for (lane, root) in [("checkout", &repository), ("absent", &absent_root)] {
        absent(&absent_historical);
        verify_checkout_historical(
            &checkout_historical,
            expect_absent,
            source_seals.get(HISTORICAL),
        );
        for (name, bound) in &source_seals {
            assert_eq!(
                seal(&repository.join(name)),
                *bound,
                "checkout changed: {name}"
            );
        }
        for (name, bound) in &copied_seals {
            assert_eq!(
                seal(&absent_root.join(name)),
                *bound,
                "copy changed: {name}"
            );
        }
        for (path, bound) in &executable_seals {
            assert_eq!(seal(path), *bound, "executable changed: {}", path.display());
        }
        let output = proof.join(format!("native-{lane}"));
        let trace = proof.join(format!("native-{lane}.exec"));
        let stdout = proof.join(format!("native-{lane}.stdout"));
        let stderr = proof.join(format!("native-{lane}.stderr"));
        let status = NativeRunner
            .run_to_files(
                &run_spec(&checker, &probe, &output, root, &trace),
                &stdout,
                &stderr,
            )
            .unwrap();
        fs::write(
            proof.join(format!("native-{lane}-status.json")),
            serde_json::to_vec_pretty(&json!({"exit_code":status.code})).unwrap(),
        )
        .unwrap();
        assert_eq!(
            status.code,
            0,
            "{lane}: {}",
            String::from_utf8_lossy(&fs::read(&stderr).unwrap())
        );
        let checker_text = checker.to_str().unwrap().to_owned();
        let probe_text = probe.to_str().unwrap().to_owned();
        let output_text = output.to_str().unwrap().to_owned();
        let expected = vec![
            (
                checker_text.clone(),
                vec![
                    checker_text,
                    "--binary".into(),
                    probe_text.clone(),
                    "--output".into(),
                    output_text.clone(),
                ],
            ),
            (
                probe_text.clone(),
                vec![
                    probe_text,
                    format!("{output_text}/request.jsonl"),
                    format!("{output_text}/actual.jsonl"),
                ],
            ),
        ];
        let observed = audited_exec(&trace);
        fs::write(
            proof.join(format!("native-{lane}-exec-audit.json")),
            serde_json::to_vec_pretty(&json!({"observed":&observed,"expected":&expected})).unwrap(),
        )
        .unwrap();
        assert_eq!(observed, expected, "{lane}: unexpected executable or argv");
        let report = value(&output.join("report.json"));
        assert_eq!(report["frames"], 24_585);
        assert_eq!(report["numeric_tolerance"], json!(1e-10));
        assert_eq!(report["device_validation"], false);
        assert_eq!(report["state_coverage"], manifest["state_coverage"]);
        assert_eq!(value(&output.join("original-only-manifest.json")), manifest);
        assert_eq!(value(&output.join("process.json"))["exit_code"], 0);
        for file in ["request.jsonl", "reference.jsonl"] {
            assert_eq!(
                sha_file(&output.join(file)),
                manifest["files"][file]["sha256"].as_str().unwrap()
            );
        }
        assert!(output.join("actual.jsonl").metadata().unwrap().len() > 0);
        if let Some(first) = &first_output {
            for file in [
                "report.json",
                "request.jsonl",
                "reference.jsonl",
                "actual.jsonl",
                "stdout.log",
                "stderr.log",
                "process.json",
                "original-only-manifest.json",
            ] {
                same_bytes(&first.join(file), &output.join(file));
            }
            same_bytes(&proof.join("native-checkout.stdout"), &stdout);
            same_bytes(&proof.join("native-checkout.stderr"), &stderr);
        } else {
            first_output = Some(output.clone());
        }
        absent(&absent_historical);
        verify_checkout_historical(
            &checkout_historical,
            expect_absent,
            source_seals.get(HISTORICAL),
        );
        for (name, bound) in &source_seals {
            assert_eq!(
                seal(&repository.join(name)),
                *bound,
                "checkout changed: {name}"
            );
        }
        for (name, bound) in &copied_seals {
            assert_eq!(
                seal(&absent_root.join(name)),
                *bound,
                "copy changed: {name}"
            );
        }
        for (path, bound) in &executable_seals {
            assert_eq!(seal(path), *bound, "executable changed: {}", path.display());
        }
    }
    let mut final_sources = BTreeMap::new();
    let mut final_copies = BTreeMap::new();
    for name in source_seals.keys() {
        final_sources.insert(name.clone(), seal(&repository.join(name)));
        if name.as_str() != HISTORICAL {
            final_copies.insert(name.clone(), seal(&absent_root.join(name)));
        }
    }
    let final_executables = BTreeMap::from([
        (checker.clone(), seal(&checker)),
        (probe.clone(), seal(&probe)),
    ]);
    assert_eq!(final_sources, source_seals);
    assert_eq!(final_copies, copied_seals);
    assert_eq!(final_executables, executable_seals);
    absent(&absent_historical);
    verify_checkout_historical(
        &checkout_historical,
        expect_absent,
        source_seals.get(HISTORICAL),
    );
    fs::write(
        proof.join("inputs-after.json"),
        serde_json::to_vec_pretty(&json!({"executables":&final_executables,"checkout_sources":&final_sources,"absent_sources":&final_copies,"checkout_historical_absent":expect_absent})).unwrap(),
    )
    .unwrap();
    fs::write(
        proof.join("comparison.json"),
        serde_json::to_vec_pretty(&json!({"pass":true,"frames":24585,"scenarios":47,"same_executable_inodes_and_bytes":true,"copied_source_files":27,"successful_execs_per_lane":2,"checkout_historical_absent":expect_absent})).unwrap(),
    )
    .unwrap();
    // CLI help/error observations have a separate cloud capture. This gate binds
    // the full corpus and same-ELF source-absence behavior in both root lanes.
}
