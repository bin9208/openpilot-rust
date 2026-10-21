//! Hosted only: fixed independent fixture, identical ELFs, explicit source absence.
#[path = "support/driving_cli.rs"]
mod cli;
use openpilot_dev_tools::{
    driving_daemon_check::source_policy::RETIRED, driving_daemon_reference::fixture::Fixture,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs::{self, File},
    io::Read,
    os::unix::{
        ffi::OsStrExt,
        fs::{symlink, MetadataExt},
    },
    path::{Path, PathBuf},
    process::Command,
};
fn required(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("cloud must set {name}")))
}
fn sha(path: &Path) -> String {
    let mut f = File::open(path).unwrap();
    let mut h = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let n = f.read(&mut bytes).unwrap();
        if n == 0 {
            break;
        }
        h.update(&bytes[..n]);
    }
    format!("{:x}", h.finalize())
}
fn excluded(relative: &Path) -> bool {
    relative.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some(
                ".git"
                    | ".analysis"
                    | ".omo"
                    | "target"
                    | "__pycache__"
                    | "node_modules"
                    | ".venv"
                    | "cache"
            )
        )
    }) || RETIRED.iter().any(|(name, _)| relative == Path::new(name))
}
fn copy_tree(
    from: &Path,
    to: &Path,
    relative: &Path,
    budget: &mut u64,
    files: &mut BTreeMap<String, String>,
) {
    fs::create_dir(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let rel = relative.join(entry.file_name());
        if excluded(&rel) {
            continue;
        }
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let kind = entry.file_type().unwrap();
        *budget = budget
            .checked_sub(4096)
            .expect("private entry budget exhausted");
        if kind.is_dir() {
            copy_tree(&src, &dst, &rel, budget, files);
        } else if kind.is_symlink() {
            let target = fs::read_link(&src).unwrap();
            symlink(&target, &dst).unwrap();
            files.insert(
                rel.to_str().unwrap().into(),
                format!(
                    "symlink:{}",
                    target
                        .as_os_str()
                        .as_bytes()
                        .iter()
                        .map(|v| format!("{v:02x}"))
                        .collect::<String>()
                ),
            );
        } else {
            assert!(kind.is_file());
            let before = fs::metadata(&src).unwrap();
            *budget = budget
                .checked_sub(before.len())
                .expect("private data budget exhausted");
            let hash = sha(&src);
            let mut input = File::open(&src).unwrap().take(before.len() + 1);
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&dst)
                .unwrap();
            assert_eq!(
                std::io::copy(&mut input, &mut output).unwrap(),
                before.len()
            );
            fs::set_permissions(&dst, before.permissions()).unwrap();
            assert_eq!(sha(&src), hash);
            assert_eq!(sha(&dst), hash);
            files.insert(rel.to_str().unwrap().into(), hash);
        }
    }
    fs::set_permissions(to, fs::metadata(from).unwrap().permissions()).unwrap();
}
fn snapshot(root: &Path) -> BTreeMap<String, String> {
    fn visit(root: &Path, relative: &Path, out: &mut BTreeMap<String, String>) {
        for e in fs::read_dir(root.join(relative)).unwrap() {
            let e = e.unwrap();
            let rel = relative.join(e.file_name());
            if excluded(&rel) {
                continue;
            }
            let kind = e.file_type().unwrap();
            if kind.is_dir() {
                visit(root, &rel, out);
            } else if kind.is_symlink() {
                let target = fs::read_link(e.path()).unwrap();
                out.insert(
                    rel.to_str().unwrap().into(),
                    format!(
                        "symlink:{}",
                        target
                            .as_os_str()
                            .as_bytes()
                            .iter()
                            .map(|v| format!("{v:02x}"))
                            .collect::<String>()
                    ),
                );
            } else {
                assert!(kind.is_file());
                out.insert(rel.to_str().unwrap().into(), sha(&e.path()));
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, Path::new(""), &mut result);
    result
}
fn absent(root: &Path) {
    for (name, _) in RETIRED {
        assert!(
            matches!(fs::symlink_metadata(root.join(name)),Err(e)if e.kind()==std::io::ErrorKind::NotFound),
            "retired source present: {name}"
        );
    }
}
struct Held {
    file: File,
    identity: (u64, u64, u64),
    digest: String,
}
impl Held {
    fn open(path: &Path) -> Self {
        let meta = fs::symlink_metadata(path).unwrap();
        assert!(meta.is_file() && !meta.file_type().is_symlink());
        let mut file = File::open(path).unwrap();
        let mut header = [0; 6];
        file.read_exact(&mut header).unwrap();
        assert_eq!(&header, b"\x7fELF\x02\x01");
        Self {
            file,
            identity: (meta.dev(), meta.ino(), meta.len()),
            digest: sha(path),
        }
    }
    fn verify(&self, path: &Path) {
        let held = self.file.metadata().unwrap();
        let named = fs::symlink_metadata(path).unwrap();
        assert!(named.is_file() && !named.file_type().is_symlink());
        assert_eq!((held.dev(), held.ino(), held.len()), self.identity);
        assert_eq!((named.dev(), named.ino(), named.len()), self.identity);
        assert_eq!(sha(path), self.digest);
    }
}
fn arg(args: &mut Vec<OsString>, key: &str, value: &Path) {
    args.extend([key.into(), value.as_os_str().to_owned()]);
}
fn base(fixture: &Fixture, root: &Path, policy: &str, output: &Path) -> Vec<OsString> {
    let mut args = vec![
        "--source-policy".into(),
        policy.into(),
        "--original-manifest-sha256".into(),
        fixture.original_manifest_sha256().into(),
    ];
    for (key, path) in [
        ("--catalog", fixture.catalog()),
        ("--original-dir", fixture.original_directory()),
        ("--root", root.to_owned()),
        ("--output", output.to_owned()),
    ] {
        arg(&mut args, key, &path);
    }
    args
}
fn stable(report: &Value) -> Value {
    let mut result = report.clone();
    for row in result["runs"].as_array_mut().unwrap() {
        if let Some(log) = row.as_object_mut().unwrap().remove("logging") {
            if !log.is_null() {
                assert_eq!(log["result"], "pass");
                assert_eq!(log["source_calls"]["result"], "pass");
                assert_eq!(log["collector_exit"], 0);
                assert!(log["runtime_timing_events"].as_u64().unwrap() > 0);
                row["source_calls"] = log["source_calls"].clone();
            }
        }
    }
    result
}
#[test]
fn same_elf_complete_fixture_survives_three_explicit_python_absences() {
    let repo = required("DRIVING_DAEMON_ROOT").canonicalize().unwrap();
    let evidence = required("DRIVING_RETIREMENT_OUTPUT");
    assert!(!evidence.starts_with(&repo));
    fs::create_dir(&evidence).unwrap();
    let private = evidence.join("private-root");
    let fixture = Fixture::open(
        &required("DRIVING_FIXTURE_DIRECTORY"),
        &env::var("DRIVING_FIXTURE_MANIFEST_SHA256").unwrap(),
    )
    .unwrap();
    let source = env::var("EXPECTED_SOURCE").unwrap();
    let checkout = Command::new("git")
        .args(["-C"])
        .arg(&repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(checkout.status.success());
    assert_eq!(String::from_utf8(checkout.stdout).unwrap().trim(), source);
    let expect_absent = match env::var("DRIVING_RETIREMENT_EXPECT_ABSENT")
        .unwrap()
        .as_str()
    {
        "0" => false,
        "1" => true,
        _ => panic!("explicit absence mode required"),
    };
    if expect_absent {
        absent(&repo);
    } else {
        for (name, pin) in RETIRED {
            assert_eq!(sha(&repo.join(name)), pin);
        }
    }
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-driving-daemon"));
    let fault_checker = PathBuf::from(env!("CARGO_BIN_EXE_check-model-logging-faults"));
    let daemon = required("DRIVING_DAEMON_NATIVE_BINARY");
    let peer = required("DRIVING_DAEMON_PEER");
    let collector = required("DRIVING_DAEMON_COLLECTOR");
    let fault = required("DRIVING_LOGGING_FAULT_BINARY");
    let seals = [&checker, &fault_checker, &daemon, &peer, &collector, &fault]
        .into_iter()
        .map(|p| (p.clone(), Held::open(p)))
        .collect::<BTreeMap<_, _>>();
    openpilot_disk_space::inspect(&evidence, 3 * 1024 * 1024 * 1024)
        .unwrap()
        .require()
        .unwrap();
    let mut files = BTreeMap::new();
    copy_tree(
        &repo,
        &private,
        Path::new(""),
        &mut (3 * 1024 * 1024 * 1024),
        &mut files,
    );
    assert_eq!(snapshot(&private), files);
    assert_eq!(snapshot(&repo), files);
    absent(&private);
    // Only source commit identification uses shared read-only Git metadata. No
    // historical source is restored, imported or executed in either candidate root.
    let git = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["rev-parse", "--absolute-git-dir"])
        .output()
        .unwrap();
    assert!(git.status.success());
    let gitdir = String::from_utf8(git.stdout).unwrap();
    fs::write(private.join(".git"), format!("gitdir: {}\n", gitdir.trim())).unwrap();
    fs::write(evidence.join("source-copy.json"),serde_json::to_vec_pretty(&json!({"source":source,"files":files,"private_git_metadata_reference":gitdir.trim(),"historical_sources_restored":false})).unwrap()).unwrap();
    let mut comparisons = Vec::new();
    for (name, root, policy) in [
        (
            "checkout",
            &repo,
            if expect_absent {
                "retired-driving"
            } else {
                "present"
            },
        ),
        ("private", &private, "retired-driving"),
    ] {
        let mut pair = Vec::new();
        for (logging, suffix) in [(false, "model"), (true, "collector")] {
            let out = evidence.join(format!("{name}-{suffix}"));
            let mut args = base(&fixture, root, policy, &out);
            arg(&mut args, "--binary", &daemon);
            arg(&mut args, "--peer", &peer);
            if logging {
                arg(&mut args, "--collector", &collector);
            }
            let report = if name == "checkout" {
                cli::run(&checker, args, &out)
            } else {
                cli::run_at(&checker, args, &out, root)
            };
            assert_eq!(report["runs"].as_array().unwrap().len(), 6);
            assert_eq!(
                report["runs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["messages"].as_u64().unwrap())
                    .sum::<u64>(),
                114
            );
            pair.push(stable(&report));
            fixture.verify().unwrap();
        }
        let out = evidence.join(format!("{name}-fault"));
        let mut args = base(&fixture, root, policy, &out);
        for (key, path) in [
            ("--fault-binary", &fault),
            ("--peer", &peer),
            ("--collector", &collector),
        ] {
            arg(&mut args, key, path);
        }
        let report = cli::run_at(&fault_checker, args, &out, root);
        assert_eq!(report["result"], "pass");
        assert_eq!(report["ordinary_exit"], 1);
        assert_eq!(report["native"].as_array().unwrap().len(), 2);
        for run in report["native"].as_array().unwrap() {
            assert_eq!(run["model_messages"], 33);
            assert_eq!(run["next_interval_frames"], run["expected_next_frames"]);
        }
        pair.push(json!({"source":report["source"],"modes":report["native"].as_array().unwrap().iter().map(|r|r["mode"].clone()).collect::<Vec<_>>(),"ordinary_exit":report["ordinary_exit"]}));
        comparisons.push(pair);
        fixture.verify().unwrap();
        for (path, seal) in &seals {
            seal.verify(path);
        }
        absent(&private);
        assert_eq!(snapshot(&private), files);
        assert_eq!(snapshot(&repo), files);
    }
    assert_eq!(comparisons[0], comparisons[1]);
    for (i, (relative, _)) in RETIRED.iter().enumerate() {
        let path = private.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for link in [false, true] {
            if link {
                symlink("missing-retired-target", &path).unwrap();
            } else {
                fs::write(&path, b"reappeared").unwrap();
            }
            let out = evidence.join(format!("reappearance-{i}-{link}"));
            let mut args = base(&fixture, &private, "retired-driving", &out);
            arg(&mut args, "--binary", &daemon);
            arg(&mut args, "--peer", &peer);
            let result = Command::new(&checker)
                .args(&args)
                .current_dir(&private)
                .env_remove("PYTHONPATH")
                .env_remove("PYTHONHOME")
                .output()
                .unwrap();
            fs::write(evidence.join(format!("reappearance-{i}-{link}.json")),serde_json::to_vec_pretty(&json!({"status":result.status.code(),"stderr_hex":result.stderr.iter().map(|b|format!("{b:02x}")).collect::<String>(),"report_exists":out.join("report.json").exists()})).unwrap()).unwrap();
            assert_eq!(result.status.code(), Some(1));
            assert!(
                String::from_utf8_lossy(&result.stderr).contains("retired driving source present")
            );
            assert!(!out.join("report.json").exists());
            fs::remove_file(&path).unwrap();
        }
    }
    absent(&private);
    fixture.verify().unwrap();
    for (path, seal) in &seals {
        seal.verify(path);
    }
    assert_eq!(snapshot(&private), files);
    assert_eq!(snapshot(&repo), files);
    fs::write(evidence.join("summary.json"),serde_json::to_vec_pretty(&json!({"passed":true,"source":source,"same_elf":seals.iter().map(|(p,s)|(p.display().to_string(),s.digest.clone())).collect::<BTreeMap<_,_>>(),"lanes":2,"model_publications_per_lane":114,"collector_publications_per_lane":114,"linked_fault_modes_per_lane":2,"reappearance_failures":6,"python_sources_restored":false})).unwrap()).unwrap();
}
