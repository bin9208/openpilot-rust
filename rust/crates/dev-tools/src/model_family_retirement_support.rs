//! Private integration-test support; this module is included only by the hosted gate.
use openpilot_dev_tools::qa_process::{Captured, CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{symlink, MetadataExt},
    },
    path::{Component, Path, PathBuf},
    time::Duration,
};

pub fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} required"))
        .into()
}
pub fn text(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} required"))
}
pub fn bytes_sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn pin(path: &Path) -> Value {
    let before = fs::symlink_metadata(path).unwrap();
    assert!(before.is_file() && !before.file_type().is_symlink());
    assert!(
        before.len() <= 1024 * 1024 * 1024,
        "input exceeds gate bound"
    );
    let mut file = File::open(path).unwrap();
    let identity = |m: &fs::Metadata| {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    };
    assert_eq!(identity(&before), identity(&file.metadata().unwrap()));
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    let mut total = 0u64;
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        total += count as u64;
        assert!(total <= before.len());
        digest.update(&buffer[..count]);
    }
    assert_eq!(total, before.len());
    assert_eq!(identity(&before), identity(&file.metadata().unwrap()));
    assert_eq!(
        identity(&before),
        identity(&fs::symlink_metadata(path).unwrap())
    );
    json!({"bytes":total,"sha256":format!("{:x}",digest.finalize())})
}
pub struct Held {
    path: PathBuf,
    file: File,
    identity: (u64, u64, u64, i64, i64, i64, i64),
    pub seal: Value,
}
fn identity(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
impl Held {
    pub fn open(path: &Path) -> Self {
        let seal = pin(path);
        let file = File::open(path).unwrap();
        let result = Self {
            path: path.to_owned(),
            identity: identity(&file.metadata().unwrap()),
            file,
            seal,
        };
        result.verify();
        result
    }
    pub fn verify(&self) {
        assert_eq!(identity(&self.file.metadata().unwrap()), self.identity);
        assert_eq!(
            identity(&fs::symlink_metadata(&self.path).unwrap()),
            self.identity
        );
        assert_eq!(
            pin(&self.path),
            self.seal,
            "held input changed: {}",
            self.path.display()
        );
    }
    pub fn evidence(&self) -> Value {
        json!({"path":self.path,"seal":self.seal,"identity":self.identity})
    }
}
pub fn read_json(path: &Path) -> Value {
    assert!(fs::symlink_metadata(path).unwrap().len() <= 16 * 1024 * 1024);
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
pub fn write_json(path: &Path, value: &Value) {
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    output
        .write_all(&serde_json::to_vec_pretty(value).unwrap())
        .unwrap();
    output.write_all(b"\n").unwrap();
    output.flush().unwrap();
}
pub fn run(
    cli: &Path,
    args: &[OsString],
    proof: &Path,
    receipt: Option<&Path>,
    negative: bool,
) -> Captured {
    fs::create_dir(proof).unwrap();
    let argv = std::iter::once(cli.as_os_str().to_owned())
        .chain(args.iter().cloned())
        .collect();
    let mut spec =
        CommandSpec::new_os(argv, Duration::from_secs(if negative { 120 } else { 1200 }));
    spec.stdout_limit = 1024 * 1024;
    if cli == Path::new("/usr/bin/git") && args.iter().any(|arg| arg == "ls-files") {
        // Metadata inventory only; each tested tool retains its 1 MiB limit.
        spec.stdout_limit = 4 * 1024 * 1024;
    }
    spec.stderr_limit = 1024 * 1024;
    spec.env_remove = vec!["PYTHONPATH".into(), "PYTHONHOME".into()];
    spec.env
        .insert("PATH".to_owned(), "/nonexistent".to_owned());
    if let Some(receipt) = receipt {
        spec.env.insert(
            "MODEL_FAMILY_CURRENT_CATALOG_RECEIPT".to_owned(),
            receipt.to_str().unwrap().to_owned(),
        );
        spec.env.insert(
            "MODEL_FAMILY_CURRENT_CATALOG_RECEIPT_SHA256".to_owned(),
            pin(receipt)["sha256"].as_str().unwrap().to_owned(),
        );
    }
    let result = NativeRunner.run_to_files(&spec, &proof.join("stdout"), &proof.join("stderr"));
    let stdout = fs::read(proof.join("stdout")).unwrap_or_default();
    let stderr = fs::read(proof.join("stderr")).unwrap_or_default();
    write_json(
        &proof.join("invocation.json"),
        &json!({
        "argv_bytes":spec.effective_argv().iter().map(|a|a.as_bytes().to_vec()).collect::<Vec<_>>(),
        "status":result.as_ref().ok().map(|r|r.code),"error":result.as_ref().err().map(ToString::to_string),
        "timeout_seconds":spec.timeout.as_secs(),"stream_limit_bytes":spec.stdout_limit,
        "stdout":{"bytes":stdout.len(),"sha256":bytes_sha(&stdout)},
        "stderr":{"bytes":stderr.len(),"sha256":bytes_sha(&stderr)},
        "environment_overlay":spec.env,"removed_environment":["PYTHONPATH","PYTHONHOME"]}),
    );
    Captured {
        code: result
            .expect("native command failed; receipt retained")
            .code,
        stdout,
        stderr,
    }
}
fn stdout_matches_runs(stdout: &[u8], runs: &[Value], count: usize) -> Result<(), String> {
    if runs.len() != count {
        return Err("report case count changed".into());
    }
    let body = stdout
        .strip_suffix(b"\n")
        .ok_or("stdout JSONL is not newline terminated")?;
    let rows = body
        .split(|&b| b == b'\n')
        .map(|line| {
            serde_json::from_slice::<Value>(line).map_err(|e| format!("invalid stdout JSONL: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if rows.as_slice() != runs {
        return Err("complete stdout JSONL differs from report runs".into());
    }
    Ok(())
}
pub fn success(result: &Captured, output: &Path, lines: usize) -> Value {
    success_rows(result, output, lines, false)
}
pub fn success_usbgpu(result: &Captured, output: &Path) -> Value {
    success_rows(result, output, 6, true)
}
fn success_rows(result: &Captured, output: &Path, lines: usize, usb_summary: bool) -> Value {
    assert_eq!(
        result.code,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report = read_json(&output.join("report.json"));
    let runs = report["runs"].as_array().unwrap();
    // The original USB CLI publishes three summary fields per case; its report
    // retains the full packets, worker lifecycle and process receipts separately.
    let summaries;
    let stdout_rows = if usb_summary {
        summaries = runs
            .iter()
            .map(|row| json!({"camera":row["camera"],"mode":row["mode"],"passed":row["passed"]}))
            .collect::<Vec<_>>();
        &summaries
    } else {
        runs
    };
    stdout_matches_runs(&result.stdout, stdout_rows, lines)
        .expect("native stdout must publish exactly every aggregate report row");
    report
}
pub fn pair(args: &mut Vec<OsString>, key: &str, value: impl AsRef<OsStr>) {
    args.push(key.into());
    args.push(value.as_ref().to_owned());
}
pub fn replace(args: &mut [OsString], key: &str, value: impl AsRef<OsStr>) {
    let position = args.iter().position(|v| v == key).unwrap();
    args[position + 1] = value.as_ref().to_owned();
}
pub fn absence(root: &Path, retired: &[&str]) {
    for name in retired {
        let result = fs::symlink_metadata(root.join("rust/tools").join(name));
        assert!(
            result.is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            "retired path present: {name}"
        );
    }
}
pub struct OwnedPath {
    path: PathBuf,
    identity: (u64, u64),
    _file: File,
}
impl OwnedPath {
    pub fn reappearance(path: &Path, target: Option<&Path>) -> Self {
        if let Some(target) = target {
            symlink(target, path).unwrap();
        } else {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap()
                .write_all(b"controlled retired source reappearance")
                .unwrap();
        }
        let m = fs::symlink_metadata(path).unwrap();
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .unwrap();
        let file = File::from(fd);
        let opened = file.metadata().unwrap();
        assert_eq!((m.dev(), m.ino()), (opened.dev(), opened.ino()));
        Self {
            path: path.to_owned(),
            identity: (m.dev(), m.ino()),
            _file: file,
        }
    }
}
impl Drop for OwnedPath {
    fn drop(&mut self) {
        if let Ok(m) = fs::symlink_metadata(&self.path) {
            if (m.dev(), m.ino()) == self.identity {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}
pub fn raw_alias(lane: &Path) -> (PathBuf, OwnedPath) {
    let path = lane.join(OsString::from_vec(b"raw-\xff".to_vec()));
    let owner = OwnedPath::reappearance(&path, Some(lane));
    write_json(
        &lane.join("raw-output-alias.json"),
        &json!({"alias_path_bytes":path.as_os_str().as_bytes(),"target":lane,"scope":"actual CLI argv uses raw byte alias to real owned output"}),
    );
    (path, owner)
}
pub fn copy_source(root: &Path, private: &Path, proof: &Path) -> BTreeMap<PathBuf, Value> {
    fs::create_dir(private).unwrap();
    let args = [
        "-C".into(),
        root.as_os_str().to_owned(),
        "ls-files".into(),
        "-z".into(),
    ];
    let result = run(
        Path::new("/usr/bin/git"),
        &args,
        &proof.join("git-tracked-source"),
        None,
        true,
    );
    assert_eq!(
        result.code,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut copied = BTreeMap::new();
    for name in result.stdout.split(|&b| b == 0).filter(|n| !n.is_empty()) {
        let name = PathBuf::from(OsString::from_vec(name.to_vec()));
        assert!(name.components().all(|c| matches!(c, Component::Normal(_))));
        assert!(copied.len() < 30000);
        let source = root.join(&name);
        let target = private.join(&name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let m = fs::symlink_metadata(&source).unwrap();
        let seal = if m.file_type().is_symlink() {
            let link = fs::read_link(&source).unwrap();
            symlink(&link, &target).unwrap();
            json!({"symlink_bytes":link.as_os_str().as_bytes()})
        } else {
            let seal = pin(&source);
            fs::copy(&source, &target).unwrap();
            assert_eq!(pin(&target), seal);
            seal
        };
        assert!(copied.insert(name, seal).is_none());
    }
    fn copy_git(source: &Path, target: &Path, bytes: &mut u64, count: &mut usize) {
        let m = fs::symlink_metadata(source).unwrap();
        assert!(
            !m.file_type().is_symlink(),
            "Git metadata alias is not an owned independent copy"
        );
        if m.is_dir() {
            fs::create_dir(target).unwrap();
            for entry in fs::read_dir(source).unwrap() {
                let e = entry.unwrap();
                copy_git(&e.path(), &target.join(e.file_name()), bytes, count);
            }
        } else {
            assert!(m.is_file());
            *bytes = bytes.checked_add(m.len()).unwrap();
            *count += 1;
            assert!(
                *bytes <= 2 * 1024 * 1024 * 1024 && *count <= 100000,
                "Git metadata copy exceeds bound"
            );
            fs::copy(source, target).unwrap();
        }
    }
    let mut bytes = 0;
    let mut count = 0;
    copy_git(
        &root.join(".git"),
        &private.join(".git"),
        &mut bytes,
        &mut count,
    );
    write_json(
        &proof.join("source-copy.json"),
        &serde_json::to_value(&copied).unwrap(),
    );
    let head = run(
        Path::new("/usr/bin/git"),
        &[
            "-C".into(),
            private.as_os_str().to_owned(),
            "rev-parse".into(),
            "HEAD".into(),
        ],
        &proof.join("private-git-identity"),
        None,
        true,
    );
    assert_eq!(head.code, 0);
    assert_eq!(
        std::str::from_utf8(&head.stdout).unwrap().trim(),
        text("EXPECTED_SOURCE")
    );
    copied
}
pub fn verify_source(root: &Path, copied: &BTreeMap<PathBuf, Value>) {
    for (name, seal) in copied {
        let path = root.join(name);
        if let Some(bytes) = seal.get("symlink_bytes") {
            assert_eq!(
                json!(fs::read_link(path).unwrap().as_os_str().as_bytes()),
                *bytes
            );
        } else {
            assert_eq!(pin(&path), *seal, "source changed: {}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_stdout_rows_must_equal_the_aggregate_report_in_order() {
        let runs = vec![
            json!({"case":1,"passed":true}),
            json!({"case":2,"passed":true}),
        ];
        let valid = b"{\"passed\":true,\"case\":1}\n{\"case\":2,\"passed\":true}\n";
        stdout_matches_runs(valid, &runs, 2).unwrap();
        for invalid in [
            b"PASS\nPASS\n".as_slice(),
            b"{\"case\":1,\"passed\":true}\n{\"case\":1,\"passed\":true}\n",
            b"{\"case\":2,\"passed\":true}\n{\"case\":1,\"passed\":true}\n",
            b"{\"case\":1,\"passed\":true}\n{\"case\":2,\"passed\":false}\n",
            b"{\"case\":1,\"passed\":true}\n{\"case\":2,\"passed\":true}",
            b"{\"case\":1,\"passed\":true}\n{\"case\":2,\"passed\":true}\n\n",
            b"{\"case\":1,\"passed\":true}\n{\"case\":2,\"passed\":true}\n{}\n",
            b"{\"case\":1,\"passed\":true}\n{\"case\":2,\"passed\":tru\n",
            b"\xff\n{}\n",
        ] {
            assert!(stdout_matches_runs(invalid, &runs, 2).is_err());
        }
        assert!(stdout_matches_runs(valid, &runs, 3).is_err());
    }
    #[test]
    fn usb_stdout_preserves_original_summaries_without_discarding_full_reports() {
        let root = tempfile::tempdir().unwrap();
        let report: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/model-family-original/usbgpu/original/report.json"
        ))
        .unwrap();
        let stdout = include_bytes!(
            "../tests/fixtures/model-family-original/usbgpu/original/capture-evidence/checker.stdout"
        );
        write_json(&root.path().join("report.json"), &report);
        let result = Captured {
            code: 0,
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
        };
        assert_eq!(success_usbgpu(&result, root.path()), report);
        assert!(std::panic::catch_unwind(|| success(&result, root.path(), 6)).is_err());
        let rows = stdout
            .split(|&byte| byte == b'\n')
            .filter(|row| !row.is_empty())
            .map(|row| serde_json::from_slice::<Value>(row).unwrap())
            .collect::<Vec<_>>();
        for field in ["camera", "mode", "passed", "unexpected"] {
            let mut changed = rows.clone();
            changed[0][field] = Value::Null;
            let mut bytes = Vec::new();
            for row in changed {
                serde_json::to_writer(&mut bytes, &row).unwrap();
                bytes.push(b'\n');
            }
            let invalid = Captured {
                code: 0,
                stdout: bytes,
                stderr: Vec::new(),
            };
            assert!(std::panic::catch_unwind(|| success_usbgpu(&invalid, root.path())).is_err());
        }
    }
    #[test]
    fn accepted_report_seal_rejects_later_traffic_mutation() {
        let root = tempfile::tempdir().unwrap();
        let report = root.path().join("report.json");
        fs::write(&report, b"{\"passed\":true}\n").unwrap();
        let accepted = Held::open(&report);
        fs::write(&report, b"{\"passed\":null}\n").unwrap();
        assert!(std::panic::catch_unwind(|| accepted.verify()).is_err());
    }
    #[test]
    fn reappearance_owner_preserves_foreign_replacement_and_removes_owned_alias() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("retired.py");
        let owner = OwnedPath::reappearance(&path, None);
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"foreign").unwrap();
        drop(owner);
        assert_eq!(fs::read(&path).unwrap(), b"foreign");
        let alias = root.path().join("alias");
        let owner = OwnedPath::reappearance(&alias, Some(root.path()));
        drop(owner);
        assert!(fs::symlink_metadata(alias).is_err());
    }
    #[test]
    fn held_same_bytes_replacement_is_not_the_same_elf_input() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("elf");
        fs::write(&path, b"held bytes").unwrap();
        let held = Held::open(&path);
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"held bytes").unwrap();
        assert!(std::panic::catch_unwind(|| held.verify()).is_err());
    }
}
