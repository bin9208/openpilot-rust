//! Actions-only real CLI rejection controls; no source retirement/full absent run.
use super::*;
use sha2::{Digest, Sha256};
use std::{io, os::unix::fs::FileExt};

const LEAVES: [&str; 2] = [
    "rust/tools/check_native_logging.py",
    "rust/tools/native_logging_transport.py",
];

fn stamp(m: &fs::Metadata) -> Value {
    json!({"dev":m.dev(),"ino":m.ino(),"mode":m.mode(),"uid":m.uid(),"gid":m.gid(),
        "links":m.nlink(),"size":m.len(),"mtime":m.mtime(),"mtime_nsec":m.mtime_nsec(),
        "ctime":m.ctime(),"ctime_nsec":m.ctime_nsec()})
}

struct Held {
    path: PathBuf,
    file: File,
    before: Value,
}
impl Held {
    fn open(path: PathBuf, expected: Option<&str>) -> Self {
        assert!(path.is_absolute());
        assert_eq!(path.canonicalize().unwrap(), path);
        let file = File::from(
            rustix::fs::open(
                &path,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .unwrap(),
        );
        let mut held = Self {
            path,
            file,
            before: Value::Null,
        };
        held.before = held.snapshot();
        if let Some(expected) = expected {
            assert_eq!(held.before["sha256"], expected);
        }
        held
    }
    fn snapshot(&self) -> Value {
        let before = self.file.metadata().unwrap();
        assert!(before.is_file() && before.len() <= 512 * 1024 * 1024);
        assert_eq!(
            stamp(&before),
            stamp(&fs::symlink_metadata(&self.path).unwrap())
        );
        let mut hash = Sha256::new();
        let mut offset = 0;
        let mut block = [0; 65536];
        loop {
            let size = self.file.read_at(&mut block, offset).unwrap();
            if size == 0 {
                break;
            }
            offset += size as u64;
            assert!(offset <= 512 * 1024 * 1024);
            hash.update(&block[..size]);
        }
        assert_eq!(offset, before.len());
        assert_eq!(stamp(&before), stamp(&self.file.metadata().unwrap()));
        assert_eq!(
            stamp(&before),
            stamp(&fs::symlink_metadata(&self.path).unwrap())
        );
        json!({"path":self.path,"identity":stamp(&before),"sha256":format!("{:x}",hash.finalize())})
    }
    fn verify(&self) {
        assert_eq!(self.snapshot(), self.before);
    }
    fn executable(&self) {
        let m = self.file.metadata().unwrap();
        assert_eq!(m.uid(), fs::metadata("/proc/self").unwrap().uid());
        assert!(m.mode() & 0o111 != 0 && m.mode() & 0o022 == 0);
        let mut header = [0; 64];
        self.file.read_exact_at(&mut header, 0).unwrap();
        assert_eq!(&header[..6], b"\x7fELF\x02\x01");
        assert_eq!(u16::from_le_bytes([header[18], header[19]]), 62);
        self.verify();
    }
}

fn save(path: &Path, value: &Value) {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, value).unwrap();
    file.write_all(b"\n").unwrap();
}

struct Reappearance {
    path: PathBuf,
    identity: Value,
    directory: bool,
    renamed: Option<(PathBuf, Value)>,
    removed: bool,
}
impl Reappearance {
    fn create(source: &Path, leaf: &str, kind: &str, proof: &Path) -> Self {
        let leaf = source.join(leaf);
        let mut path = leaf.clone();
        let mut renamed = None;
        let directory = kind == "directory";
        match kind {
            "regular" => fs::write(&path, b"inert reappearance control\n").unwrap(),
            "directory" => fs::create_dir(&path).unwrap(),
            "symlink" => symlink(proof.join("sentinel"), &path).unwrap(),
            "dangling-symlink" => symlink(proof.join("absent-target"), &path).unwrap(),
            "parent-symlink" => {
                path = source.join("rust/tools");
                let saved = source.join("rust/tools-absence-owned");
                assert!(fs::symlink_metadata(&saved)
                    .is_err_and(|e| e.kind() == io::ErrorKind::NotFound));
                let original = stamp(&fs::symlink_metadata(&path).unwrap());
                fs::rename(&path, &saved).unwrap();
                symlink(&saved, &path).unwrap();
                renamed = Some((saved, original));
            }
            _ => panic!("unknown node control"),
        }
        let identity = stamp(&fs::symlink_metadata(&path).unwrap());
        Self {
            path,
            identity,
            directory,
            renamed,
            removed: false,
        }
    }
    fn restore(&mut self) -> io::Result<()> {
        if self.removed {
            return Ok(());
        }
        if stamp(&fs::symlink_metadata(&self.path)?) != self.identity {
            return Err(io::Error::other(
                "source control entry replaced; preserve it",
            ));
        }
        if self.directory {
            fs::remove_dir(&self.path)?;
        } else {
            fs::remove_file(&self.path)?;
        }
        if let Some((saved, original)) = &self.renamed {
            let current = fs::symlink_metadata(saved)?;
            // rename changes ctime: compare the original directory identity only.
            if ["dev", "ino", "mode", "uid", "gid"]
                .iter()
                .any(|key| stamp(&current)[*key] != original[*key])
            {
                return Err(io::Error::other(
                    "source control parent replaced; preserve it",
                ));
            }
            if !fs::symlink_metadata(&self.path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
            {
                return Err(io::Error::other(
                    "source control parent occupied; preserve it",
                ));
            }
            fs::rename(saved, &self.path)?;
        }
        self.removed = true;
        Ok(())
    }
}
impl Drop for Reappearance {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!("source control restoration unconfirmed: {error}");
        }
    }
}

fn private_directory(path: &Path) -> (u64, u64, u32, u32, u32) {
    let m = fs::symlink_metadata(path).unwrap();
    assert!(m.is_dir() && !m.file_type().is_symlink());
    assert_eq!(path.canonicalize().unwrap(), path);
    assert_eq!(m.uid(), fs::metadata("/proc/self").unwrap().uid());
    assert_eq!(m.mode() & 0o022, 0);
    (m.dev(), m.ino(), m.mode(), m.uid(), m.gid())
}

#[test]
#[ignore = "Actions-only same current ELF rejects original reappearance before traffic"]
fn source_absence_rejects_original_nodes_before_producers() {
    for (key, expected) in [
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_REPOSITORY", "bin9208/openpilot-rust"),
        ("RUNNER_ENVIRONMENT", "github-hosted"),
        ("RUNNER_OS", "Linux"),
        ("RUNNER_ARCH", "X64"),
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "core.preloadIndex"),
        ("GIT_CONFIG_VALUE_0", "true"),
    ] {
        assert_eq!(std::env::var(key).as_deref(), Ok(expected), "{key}");
    }
    let source = PathBuf::from(
        std::env::var_os("NATIVE_LOGGING_SOURCE_ABSENCE_ROOT")
            .expect("prepared private absent copy required"),
    );
    assert!(source.is_absolute());
    let job = PathBuf::from(std::env::var_os("CI_JOB_TEMP").expect("owned job required"));
    let job_identity = private_directory(&job);
    assert_eq!(job_identity.2 & 0o077, 0);
    let copy = job.join("logging-source-absence-copy");
    assert_eq!(private_directory(&copy).2 & 0o077, 0);
    assert_eq!(source, copy.join("source"));
    let actual_source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    assert!(!source.starts_with(&actual_source));
    let evidence = PathBuf::from(
        std::env::var_os("NATIVE_LOGGING_LIFECYCLE_EVIDENCE").expect("private evidence required"),
    );
    assert_eq!(evidence, job.join("proof/logging-source-absence/lifecycle"));
    assert_eq!(private_directory(&evidence).2 & 0o077, 0);
    let owner = Held::open(job.join("owner"), None);
    assert_eq!(
        fs::read_to_string(&owner.path).unwrap(),
        format!(
            "{}:{}:{}",
            std::env::var("GITHUB_RUN_ID").unwrap(),
            std::env::var("GITHUB_RUN_ATTEMPT").unwrap(),
            std::env::var("GITHUB_JOB").unwrap()
        )
    );
    let parents = [
        source.clone(),
        source.join("rust"),
        source.join("rust/tools"),
    ];
    let identities = parents
        .iter()
        .map(|p| private_directory(p))
        .collect::<Vec<_>>();
    let check_source = || {
        for (path, expected) in parents.iter().zip(&identities) {
            assert_eq!(&private_directory(path), expected);
        }
        for name in LEAVES {
            assert!(fs::symlink_metadata(source.join(name))
                .is_err_and(|e| e.kind() == io::ErrorKind::NotFound));
        }
    };
    check_source();
    // Parent admits copy growth/Git/index/current19 before this test. This is no absent full-gate PASS.
    let binary_pin = std::env::var("NATIVE_LOGGING_RETAINED_BINARY_SHA256").unwrap();
    let context_pin = std::env::var("NATIVE_LOGGING_RETAINED_CONTEXT_SHA256").unwrap();
    let held = [
        Held::open(PathBuf::from(CHECKER), None),
        Held::open(
            provider("NATIVE_LOGGING_RETAINED_BINARY"),
            Some(&binary_pin),
        ),
        Held::open(
            provider("NATIVE_LOGGING_RETAINED_CONTEXT"),
            Some(&context_pin),
        ),
        Held::open(provider("NATIVE_LOGGING_TEST_BUILDER"), None),
    ];
    assert_eq!(
        held[0].path,
        actual_source.join("rust/target/debug/check-native-logging")
    );
    assert_eq!(
        held[1].path,
        job.join("proof/logging/bin/native_logging_probe")
    );
    assert_eq!(held[2].path, job.join("proof/logging/context/context.json"));
    assert_eq!(
        held[3].path,
        actual_source
            .join("rust/target/legacy-runtime2-builder/debug/build-native-logging-reference")
    );
    for index in [0, 1, 3] {
        held[index].executable();
    }
    let commit = std::env::var("NATIVE_LOGGING_RETAINED_SOURCE_COMMIT").unwrap();
    assert!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_eq!(std::env::var("GITHUB_SHA").unwrap(), commit);
    let suite = evidence.parent().unwrap();
    assert_eq!(private_directory(suite).2 & 0o077, 0);
    let inputs = json!({"schema":1,"source_commit":commit,"source_root":source,
        "parent_identities":identities,"providers":held.iter().map(|item| item.before.clone()).collect::<Vec<_>>(),
        "job_identity":job_identity,"owner":owner.before,"metadata_absence_positive":true,
        "absent_root_full_functional_acceptance":false,"retirement_credit":0});
    save(&suite.join("inputs.before.json"), &inputs);
    let mut rows = Vec::new();
    for mode in ["retained", "legacy"] {
        for leaf in LEAVES {
            for kind in [
                "regular",
                "directory",
                "symlink",
                "dangling-symlink",
                "parent-symlink",
            ] {
                check_source();
                let proof = root();
                fs::write(proof.join("sentinel"), b"foreign inert sentinel\n").unwrap();
                let sentinel_before = stamp(&fs::metadata(proof.join("sentinel")).unwrap());
                let mut node = Reappearance::create(&source, leaf, kind, &proof);
                let output = proof.join("output");
                let stderr = proof.join("checker.stderr");
                let mut command = Command::new(CHECKER);
                command.env_clear();
                for key in ["HOME", "PATH", "TMPDIR", "TZ", "LANG", "LC_ALL"] {
                    if let Some(value) = std::env::var_os(key) {
                        command.env(key, value);
                    }
                }
                command.envs([
                    ("GIT_CONFIG_COUNT", "1"),
                    ("GIT_CONFIG_KEY_0", "core.preloadIndex"),
                    ("GIT_CONFIG_VALUE_0", "true"),
                ]);
                if mode == "retained" {
                    command.arg("retained");
                }
                command
                    .arg("--root")
                    .arg(&source)
                    .arg("--binary")
                    .arg(&held[1].path)
                    .arg("--output")
                    .arg(&output)
                    .args(["--source-policy", "absent"]);
                if mode == "retained" {
                    command
                        .arg("--context")
                        .arg(&held[2].path)
                        .arg("--source-commit")
                        .arg(&commit)
                        .arg("--binary-sha256")
                        .arg(&binary_pin);
                } else {
                    command.arg("--builder").arg(&held[3].path);
                }
                command
                    .env("NATIVE_LOGGING_QA_HOLD", "first")
                    .env("NATIVE_LOGGING_QA_RECORDS", proof.join("unexpected-child"));
                let argv = std::iter::once(command.get_program())
                    .chain(command.get_args())
                    .map(|v| v.to_str().unwrap().to_owned())
                    .collect::<Vec<_>>();
                let environment = command
                    .get_envs()
                    .map(|(key, value)| {
                        format!(
                            "{}={}",
                            key.to_str().unwrap(),
                            value.unwrap().to_str().unwrap()
                        )
                    })
                    .collect::<Vec<_>>();
                let child = command
                    .stdout(File::create(proof.join("checker.stdout")).unwrap())
                    .stderr(File::create(&stderr).unwrap())
                    .spawn()
                    .unwrap();
                let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
                let pid = child.id();
                let mut checker = Checker {
                    child: Some(child),
                    identity,
                    peer: None,
                    stderr,
                };
                assert_eq!(checker.finish().code(), Some(1));
                let expected = if kind == "parent-symlink" {
                    "logging source parent boundary invalid".to_owned()
                } else {
                    format!("logging retired source reappeared: {leaf}")
                };
                assert_eq!(
                    fs::read(&checker.stderr).unwrap(),
                    format!("check-native-logging: {expected}\n").as_bytes()
                );
                assert!(fs::read(proof.join("checker.stdout")).unwrap().is_empty());
                assert!(fs::symlink_metadata(&output)
                    .is_err_and(|e| e.kind() == io::ErrorKind::NotFound));
                assert!(!proof.join("unexpected-child").exists());
                assert_eq!(
                    stamp(&fs::metadata(proof.join("sentinel")).unwrap()),
                    sentinel_before
                );
                assert_eq!(
                    fs::read(proof.join("sentinel")).unwrap(),
                    b"foreign inert sentinel\n"
                );
                for item in &held {
                    item.verify();
                }
                node.restore().unwrap();
                check_source();
                let row = json!({"schema":1,"root":proof,"mode":mode,"leaf":leaf,"node":kind,
                    "controller_argv":argv,"controller_environment":environment,
                    "checker_pid":pid,"returncode":1,"expected_error":expected,"output_absent":true,
                    "producer_barrier_absent":true,"zero_child_exec_requires_external_trace":true,
                    "absent_root_full_functional_acceptance":false,"R4_acceptance":false,"retirement_credit":0});
                save(&proof.join("source-absence-case.json"), &row);
                rows.push(row);
            }
        }
    }
    assert_eq!(rows.len(), 20);
    for item in &held {
        item.verify();
    }
    owner.verify();
    assert_eq!(private_directory(&job), job_identity);
    check_source();
    let after = json!({"schema":1,"source_commit":commit,"source_root":source,
        "parent_identities":parents.iter().map(|path| private_directory(path)).collect::<Vec<_>>(),
        "providers":held.iter().map(Held::snapshot).collect::<Vec<_>>(),
        "job_identity":private_directory(&job),"owner":owner.snapshot(),"metadata_absence_positive":true,
        "absent_root_full_functional_acceptance":false,"retirement_credit":0});
    assert_eq!(after, inputs);
    save(&suite.join("inputs.after.json"), &after);
    save(
        &suite.join("complete.json"),
        &json!({"schema":1,"source_commit":commit,
        "selector":"source_absence::source_absence_rejects_original_nodes_before_producers",
        "actual_cli_rejections":20,"paths":2,"node_variants":5,"modes":["retained","legacy"],
        "providers_unchanged":true,"source_parents_unchanged":true,"outputs_absent":true,
        "zero_child_exec_requires_external_trace":true,"absent_root_full_functional_acceptance":false,
        "R4_acceptance":false,"retirement_credit":0,"cases":rows}),
    );
}
