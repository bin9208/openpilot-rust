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
// The live collector validates each source call's level/text, order and multiset.
// This exact camera-wait DEBUG call may repeat with host scheduling.
fn source_call_semantics(log: &Value, records: &Value) -> Value {
    let pid = log["pids"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .as_u64()
        .unwrap();
    let actual = records
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["process"].as_u64() == Some(pid) && row["msg"].is_string())
        .collect::<Vec<_>>();
    assert_eq!(
        actual.len() as u64,
        log["source_calls"]["compared_records"].as_u64().unwrap(),
        "raw source-call receipt must account for every string record"
    );
    let waits = actual
        .iter()
        .filter(|row| {
            row["levelnum"] == 10 && row["msg"] == "camera pair unavailable or out of sync"
        })
        .count();
    let mut calls = log["source_calls"].clone();
    // Only the cross-root projection changes. Raw counts/records stay retained
    // and are independently accounted above; all other source-call fields stay.
    calls["compared_records"] = json!(actual.len() - waits);
    calls
}
fn stable(report: &Value, output: &Path) -> Value {
    let mut result = report.clone();
    for (index, row) in result["runs"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        if let Some(log) = row.as_object_mut().unwrap().remove("logging") {
            if !log.is_null() {
                assert_eq!(log["result"], "pass");
                assert_eq!(log["source_calls"]["result"], "pass");
                assert_eq!(log["collector_exit"], 0);
                assert!(log["runtime_timing_events"].as_u64().unwrap() > 0);
                let case = [
                    "1344-dual",
                    "1344-road",
                    "1344-wide",
                    "1928-dual",
                    "1928-road",
                    "1928-wide",
                ][index];
                let records: Value = serde_json::from_slice(
                    &fs::read(output.join(case).join("logging/logMessage.json")).unwrap(),
                )
                .unwrap();
                row["source_calls"] = source_call_semantics(&log, &records);
            }
        }
    }
    result
}
#[test]
fn same_elf_complete_fixture_survives_all_retired_python_absences() {
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
            pair.push(stable(&report, &out));
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
    let rejected = reappearance_controls(&fixture, &private, &checker, &daemon, &peer, &evidence);
    assert_eq!(rejected, RETIRED.len() * 2);
    absent(&private);
    fixture.verify().unwrap();
    for (path, seal) in &seals {
        seal.verify(path);
    }
    assert_eq!(snapshot(&private), files);
    assert_eq!(snapshot(&repo), files);
    fs::write(evidence.join("summary.json"),serde_json::to_vec_pretty(&json!({"passed":true,"source":source,"same_elf":seals.iter().map(|(p,s)|(p.display().to_string(),s.digest.clone())).collect::<BTreeMap<_,_>>(),"lanes":2,"model_publications_per_lane":114,"collector_publications_per_lane":114,"linked_fault_modes_per_lane":2,"reappearance_failures":rejected,"python_sources_restored":false})).unwrap()).unwrap();
}

#[test]
fn source_call_projection_excludes_only_the_accounted_camera_wait_debug_count() {
    let core = json!({"process":7,"levelnum":30,"msg":"modeld init"});
    let wait = json!({"process":7,"levelnum":10,"msg":"camera pair unavailable or out of sync"});
    let a = json!([core, wait]);
    let b = json!([core, wait, wait]);
    let log = |count| {
        json!({"pids":[1,7],"source_calls":{"result":"pass","compared_records":count,
        "compared_lines":[226,271],"startup_order":"model load before CarParams","pending_egpu_lines":[]}})
    };
    assert_eq!(
        source_call_semantics(&log(2), &a),
        source_call_semantics(&log(3), &b)
    );
    let extra = json!([core,wait,{"process":7,"levelnum":40,"msg":"unexpected source event"}]);
    assert_ne!(
        source_call_semantics(&log(2), &a),
        source_call_semantics(&log(3), &extra)
    );
    let wrong_level =
        json!([core,{"process":7,"levelnum":20,"msg":"camera pair unavailable or out of sync"}]);
    assert_ne!(
        source_call_semantics(&log(2), &a),
        source_call_semantics(&log(2), &wrong_level)
    );
    let mut changed = log(2);
    changed["source_calls"]["compared_lines"] = json!([226, 272]);
    assert_ne!(
        source_call_semantics(&log(2), &a),
        source_call_semantics(&changed, &a)
    );
}

fn reappearance_controls(
    fixture: &Fixture,
    private: &Path,
    checker: &Path,
    daemon: &Path,
    peer: &Path,
    evidence: &Path,
) -> usize {
    let mut rejected = 0;
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
            let mut args = base(fixture, private, "retired-driving", &out);
            arg(&mut args, "--binary", daemon);
            arg(&mut args, "--peer", peer);
            let result = Command::new(checker)
                .args(&args)
                .current_dir(private)
                .env_remove("PYTHONPATH")
                .env_remove("PYTHONHOME")
                .output()
                .unwrap();
            fs::write(evidence.join(format!("reappearance-{i}-{link}.json")),serde_json::to_vec_pretty(&json!({"status":result.status.code(),"stderr_hex":result.stderr.iter().map(|b|format!("{b:02x}")).collect::<String>(),"report_exists":out.join("report.json").exists()})).unwrap()).unwrap();
            assert_eq!(result.status.code(), Some(1));
            assert!(result.stdout.is_empty());
            assert!(
                String::from_utf8_lossy(&result.stderr).contains("retired driving source present")
            );
            assert!(!out.join("report.json").exists());
            fs::remove_file(&path).unwrap();
            rejected += 1;
        }
    }
    rejected
}

#[test]
#[ignore = "explicit current source-reappearance controls; e25 history remains three-source"]
fn all_retired_source_reappearances_fail_without_model_traffic() {
    let repo = required("DRIVING_DAEMON_ROOT").canonicalize().unwrap();
    let parent = required("DRIVING_RETIREMENT_OUTPUT");
    fs::create_dir_all(&parent).unwrap();
    assert!(!parent.starts_with(&repo));
    openpilot_disk_space::inspect(&parent, 3 * 1024 * 1024 * 1024)
        .unwrap()
        .require()
        .unwrap();
    let evidence = tempfile::Builder::new()
        .prefix("retired-reappearances-")
        .tempdir_in(parent)
        .unwrap()
        .keep();
    let private = evidence.join("private-root");
    let mut files = BTreeMap::new();
    copy_tree(
        &repo,
        &private,
        Path::new(""),
        &mut (3 * 1024 * 1024 * 1024),
        &mut files,
    );
    let fixture = Fixture::open(
        &required("DRIVING_FIXTURE_DIRECTORY"),
        &env::var("DRIVING_FIXTURE_MANIFEST_SHA256").unwrap(),
    )
    .unwrap();
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-driving-daemon"));
    let daemon = required("DRIVING_DAEMON_NATIVE_BINARY");
    let peer = required("DRIVING_DAEMON_PEER");
    let held = [&checker, &daemon, &peer]
        .into_iter()
        .map(|path| (path.clone(), Held::open(path)))
        .collect::<BTreeMap<_, _>>();
    let rejected = reappearance_controls(&fixture, &private, &checker, &daemon, &peer, &evidence);
    assert_eq!(rejected, RETIRED.len() * 2);
    absent(&private);
    assert_eq!(snapshot(&private), files);
    assert_eq!(snapshot(&repo), files);
    fixture.verify().unwrap();
    for (path, seal) in &held {
        seal.verify(path);
    }
    fs::write(evidence.join("summary.json"),serde_json::to_vec_pretty(&json!({"passed":true,"reappearance_failures":rejected,"model_traffic_executed":false,"same_elf":held.iter().map(|(p,h)|(p.display().to_string(),h.digest.clone())).collect::<BTreeMap<_,_>>()})).unwrap()).unwrap();
}

const RETAINED_E25_FILES: &[(&str, &str)] = &[
    (
        "source-copy.json",
        "d0c766c2b58aee98b2f4e6ce475c158a8d3c693b914562a2389df0acc635c775",
    ),
    (
        "checkout-model/report.json",
        "a6754c9d1a96953a2e808ac8349c0caebd8dcc129464f34cb8860b3626554297",
    ),
    (
        "checkout-model.cli.json",
        "f28a38faf3655c225d88276057220944d769ba43b08e4c475547d24edd593f35",
    ),
    (
        "checkout-collector/report.json",
        "630da2b719e790828a88b1de1f7d62ea5cea28c3a931eff60d3bad18c458922b",
    ),
    (
        "checkout-collector.cli.json",
        "e840c18c83dbe9269af5416dd67e207868598529bce04f0bd9059f3dac0d73d1",
    ),
    (
        "checkout-fault/report.json",
        "3c9fa03792271e287440c9fa85a4e2e2e1e3cee3a640a29bc6a521c3d98ef437",
    ),
    (
        "checkout-fault.cli.json",
        "b494bd23bc892af9faa3af5510cb35fe6b28584b69c27b3a363184e7364263fc",
    ),
    (
        "checkout-collector/1344-dual/logging/logMessage.json",
        "344fa409506384db943857e9d62a593d5c05d6d8ee015ab87276034c83867b53",
    ),
    (
        "checkout-collector/1344-road/logging/logMessage.json",
        "750f7254d70430e38598499cba361ef9c81d1af3882867046fd07e6f7516cfe2",
    ),
    (
        "checkout-collector/1344-wide/logging/logMessage.json",
        "244f22926bae3f0526b381ae7233cc77b691587f7758d367c89e166be1363038",
    ),
    (
        "checkout-collector/1928-dual/logging/logMessage.json",
        "1aa18166d07a63a5bc7ac430a3aefdc6123a76168f3c3f73d5f1a076e42bafbd",
    ),
    (
        "checkout-collector/1928-road/logging/logMessage.json",
        "89f86e61278a8e19a086197c980d614c95e1a045580d0583831c65e9b4e5023c",
    ),
    (
        "checkout-collector/1928-wide/logging/logMessage.json",
        "60f9fde3fa2f914267fc05ca11f248a2007a3025d7c42e634be0d96b45d194a1",
    ),
    (
        "private-model/report.json",
        "a6754c9d1a96953a2e808ac8349c0caebd8dcc129464f34cb8860b3626554297",
    ),
    (
        "private-model.cli.json",
        "e1854742c03ed799a58f57b9cb9b484bf69d60687463f4d9a6793a49450b5d02",
    ),
    (
        "private-collector/report.json",
        "6724e0edb27d31434f2c5852a7111d99a45bc2bb7fe46ac9015a670bebe224b2",
    ),
    (
        "private-collector.cli.json",
        "56b83ea44e47eb83dd0eb31727f70d588acde14f7ff4813e8473f40225fb4f19",
    ),
    (
        "private-fault/report.json",
        "ca2f9bc1f901490b30a47b5da6dd3dad32942b9e36ce5ef093b07e57a1e64baa",
    ),
    (
        "private-fault.cli.json",
        "99bfff70e48c63b6782512ed18b286244d674f8b7b8d1027e9c29a31bc65e975",
    ),
    (
        "private-collector/1344-dual/logging/logMessage.json",
        "492594a3d60f373f4d7cb56a3726282cac88f5514fc2f0da127c4c64bf616db3",
    ),
    (
        "private-collector/1344-road/logging/logMessage.json",
        "90574d0d3a225921c886d85720e0ca3d0626dd98bff19936dae95481459ccbe4",
    ),
    (
        "private-collector/1344-wide/logging/logMessage.json",
        "08d088e68d73d4ecdcbee9573e3f1d2b626c1acaac99736293b073aa93150b8d",
    ),
    (
        "private-collector/1928-dual/logging/logMessage.json",
        "cd05c51da8a2979ae80deb4d3fe103c7cd6ea868407ac3664d0aac71609510d1",
    ),
    (
        "private-collector/1928-road/logging/logMessage.json",
        "26d0a46ffb46cedd3ae3aa7e802e0cfb420628fc0e1ade43439a742848d8f2db",
    ),
    (
        "private-collector/1928-wide/logging/logMessage.json",
        "09562caf5b6756c5ca1cbd355210f619f867847284d39269472f6254f979211a",
    ),
];

#[test]
#[ignore = "authenticated retained e25 data replay, no original/model/compiler execution; invoke explicitly"]
fn retained_e25_completed_traffic_reports_match_with_accounted_wait_debug_counts() {
    let root = required("DRIVING_RETAINED_E25_REPORTS");
    for (name, pin) in RETAINED_E25_FILES {
        assert_eq!(sha(&root.join(name)), *pin, "retained e25 member: {name}");
    }
    let source: Value =
        serde_json::from_slice(&fs::read(root.join("source-copy.json")).unwrap()).unwrap();
    assert_eq!(source["source"], "e25a14386582858dae25c6daca5ced3bfb4bacab");
    assert_eq!(source["historical_sources_restored"], false);
    let read = |path: &Path| serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap();
    let mut comparisons = Vec::new();
    for lane in ["checkout", "private"] {
        let mut reports = Vec::new();
        for kind in ["model", "collector", "fault"] {
            let output = root.join(format!("{lane}-{kind}"));
            let report = read(&output.join("report.json"));
            let receipt = read(&root.join(format!("{lane}-{kind}.cli.json")));
            assert_eq!(receipt["source"], source["source"]);
            assert_eq!(receipt["status"], 0);
            assert_eq!(receipt["stderr_hex"], "");
            assert_eq!(
                receipt["binary_sha256_before"],
                receipt["binary_sha256_after"]
            );
            assert_eq!(receipt["python_path_removed"], true);
            let hex = receipt["stdout_hex"].as_str().unwrap();
            let bytes = hex
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), report);
            if kind == "fault" {
                assert_eq!(report["result"], "pass");
                assert_eq!(report["ordinary_exit"], 1);
                assert_eq!(report["native"].as_array().unwrap().len(), 2);
                for run in report["native"].as_array().unwrap() {
                    assert_eq!(run["model_messages"], 33);
                    assert_eq!(run["next_interval_frames"], run["expected_next_frames"]);
                }
                reports.push(json!({"source":report["source"],"modes":report["native"].as_array().unwrap().iter().map(|r|r["mode"].clone()).collect::<Vec<_>>(),"ordinary_exit":report["ordinary_exit"]}));
            } else {
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
                reports.push(stable(&report, &output));
            }
        }
        comparisons.push(reports);
    }
    assert_eq!(comparisons[0], comparisons[1]);
    fs::write(required("DRIVING_RETIREMENT_OUTPUT").join("retained-e25-comparison.json"),serde_json::to_vec_pretty(&json!({"passed":true,"prior_run":39635596188_u64,"artifact":12365522894_u64,"source":source["source"],"selected_files":RETAINED_E25_FILES.len(),"traffic_reexecuted":false,"only_comparison_adjustment":"accounted level10 camera pair wait debug multiplicity"})).unwrap()).unwrap();
}
