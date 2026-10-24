#![cfg(target_os = "linux")]
//! Source-first candidates only: runner doubles never execute a model or helper.
use openpilot_dev_tools::{
    current_catalog_export::{
        cpu_identity, export_destinations, export_with, parse_args, pin, Options, Runtime,
        EXPORTER_SOURCES, MODELS,
    },
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{ffi::OsString, fs, os::unix::fs::symlink, path::PathBuf};

const CPU: &str = "model name : Example CPU\nflags : z a z\ncpu MHz : 100\n\n";

#[test]
fn cpu_profiles_keep_first_order_deduplicate_flags_and_ignore_frequency() {
    assert_eq!(
        cpu_identity(&format!("{CPU}{CPU}model name: Other\nflags: b a\n")).unwrap(),
        json!([
            {"model_name":"Example CPU","flags":["a","z"]},
            {"model_name":"Other","flags":["a","b"]}
        ])
    );
    assert!(cpu_identity("processor: 0\nflags: a\n").is_err());
}

#[test]
fn original_alias_and_dotdot_cases_leave_historical_fixture_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let fixture = root.join("historical");
    fs::create_dir(&fixture).unwrap();
    fs::create_dir(fixture.join("existing")).unwrap();
    let alias = root.join("alias");
    symlink(&fixture, &alias).unwrap();
    for (catalog, receipt) in [
        (alias.join("catalog"), root.join("receipt")),
        (root.join("catalog"), alias.join("receipt")),
        (fixture.join("existing/../catalog"), root.join("receipt")),
        (root.join("catalog"), fixture.join("existing/../receipt")),
        (root.join("catalog"), root.join("catalog")),
    ] {
        assert!(export_destinations(&fixture, &catalog, &receipt).is_err());
    }
    assert_eq!(fs::read_dir(fixture).unwrap().count(), 1);
}

#[test]
fn original_safe_parent_alias_returns_canonical_parent_and_absent_leaf() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let fixture = root.join("historical");
    let safe = root.join("safe");
    fs::create_dir(&fixture).unwrap();
    fs::create_dir(&safe).unwrap();
    let alias = root.join("safe-alias");
    symlink(&safe, &alias).unwrap();
    assert_eq!(
        export_destinations(&fixture, &alias.join("catalog"), &alias.join("receipt")).unwrap(),
        (safe.join("catalog"), safe.join("receipt"))
    );
}

#[test]
fn destination_rejects_existing_dangling_and_missing_parent_without_writes() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("historical");
    fs::create_dir(&fixture).unwrap();
    let receipt = temp.path().join("receipt");
    let leaf = temp.path().join("catalog");
    symlink(temp.path().join("missing"), &leaf).unwrap();
    assert!(export_destinations(&fixture, &leaf, &receipt).is_err());
    fs::remove_file(&leaf).unwrap();
    fs::write(&leaf, b"existing").unwrap();
    assert!(export_destinations(&fixture, &leaf, &receipt).is_err());
    fs::remove_file(&leaf).unwrap();
    assert!(export_destinations(&fixture, &leaf.join("child"), &receipt).is_err());
    assert!(
        export_destinations(&fixture, &temp.path().join("missing/../catalog"), &receipt).is_err()
    );
    assert!(!receipt.exists());
    assert_eq!(fs::read_dir(fixture).unwrap().count(), 0);
}

#[test]
fn pin_rejects_symlinks_and_directories_without_loading_contents() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("input");
    fs::write(&file, b"inert bytes").unwrap();
    let pinned = pin(&file).unwrap();
    assert_eq!(pinned["bytes"], 11);
    assert_eq!(pinned, pin(&file).unwrap());
    symlink(&file, temp.path().join("alias")).unwrap();
    assert!(pin(&temp.path().join("alias")).is_err());
    assert!(pin(temp.path()).is_err());
}

struct Fixture {
    _temp: tempfile::TempDir,
    options: Options,
    runtime: Runtime,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let root = base.join("repo");
        let fixture = base.join("historical");
        fs::create_dir(&root).unwrap();
        fs::create_dir_all(fixture.join("models")).unwrap();
        for name in EXPORTER_SOURCES {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"inert source pin; never imported").unwrap();
        }
        fs::write(root.join("uv.lock"), b"inert lock").unwrap();
        let mut models = serde_json::Map::new();
        for name in MODELS {
            let path = fixture.join("models").join(name);
            // Deliberately not a pickle: the wrapper must only hash this data.
            fs::write(&path, b"inert model pin").unwrap();
            models.insert(format!("models/{name}"), pin(&path).unwrap());
        }
        fs::write(
            fixture.join("fixture.json"),
            serde_json::to_vec(&json!({"files":models,"catalog_generation":"historical"})).unwrap(),
        )
        .unwrap();
        let llvm = base.join("current-libllvm");
        fs::write(&llvm, b"inert library pin; never loaded").unwrap();
        let options = Options {
            root,
            fixture: fixture.clone(),
            catalog: base.join("catalog"),
            receipt: base.join("receipt.json"),
            fixture_sha256: pin(&fixture.join("fixture.json")).unwrap()["sha256"]
                .as_str()
                .unwrap()
                .into(),
            // Keep this lexical interpreter spelling; do not resolve a venv alias.
            python: base.join("venv/bin/python"),
        };
        Self {
            _temp: temp,
            options,
            runtime: Runtime {
                dev: "CPU:LLVM".into(),
                llvm,
                cached_code_configured: false,
            },
        }
    }
}

#[derive(Clone, Copy)]
enum Effect {
    None,
    Model,
    Source,
    Lock,
    Llvm,
    CatalogSymlink,
    BadGeneration,
    PackageQueryFailure,
    HelperFailure,
    GitFailure,
    ReceiptCollision,
}
struct FakeRunner<'a> {
    fixture: &'a Fixture,
    calls: Vec<CommandSpec>,
    effect: Effect,
}
impl<'a> FakeRunner<'a> {
    fn new(fixture: &'a Fixture, effect: Effect) -> Self {
        Self {
            fixture,
            calls: Vec::new(),
            effect,
        }
    }
}
impl Runner for FakeRunner<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls.push(spec.clone());
        let index = self.calls.len();
        let options = &self.fixture.options;
        let mut code = 0;
        let stdout = match index {
            1 => {
                if matches!(self.effect, Effect::PackageQueryFailure) {
                    code = 1;
                }
                b"18.1.8-1\n".to_vec()
            }
            2 => {
                let catalog = &options.catalog;
                fs::create_dir(catalog)?;
                let temporary = catalog.join("index-data");
                fs::write(&temporary, b"{\"inert\":true}\n")?;
                let generation = pin(&temporary)?["sha256"].as_str().unwrap().to_owned();
                fs::create_dir(catalog.join(&generation))?;
                fs::rename(temporary, catalog.join(&generation).join("index.json"))?;
                fs::write(
                    catalog.join("current.json"),
                    serde_json::to_vec(&json!({"generation":generation}))?,
                )?;
                match self.effect {
                    Effect::Model => {
                        fs::write(options.fixture.join("models").join(MODELS[0]), b"changed")?
                    }
                    Effect::Source => {
                        fs::write(options.root.join(EXPORTER_SOURCES[0]), b"changed")?
                    }
                    Effect::Lock => fs::write(options.root.join("uv.lock"), b"changed")?,
                    Effect::Llvm => fs::write(&self.fixture.runtime.llvm, b"changed")?,
                    Effect::CatalogSymlink => {
                        symlink(options.root.join("uv.lock"), catalog.join("alias"))?
                    }
                    Effect::BadGeneration => {
                        fs::write(catalog.join("current.json"), b"{\"generation\":\"wrong\"}")?
                    }
                    Effect::HelperFailure => code = 7,
                    Effect::ReceiptCollision => {
                        fs::write(&options.receipt, b"keep concurrent receipt")?
                    }
                    _ => {}
                }
                Vec::new()
            }
            3 => {
                if matches!(self.effect, Effect::GitFailure) {
                    code = 1;
                }
                b"0123456789012345678901234567890123456789\n".to_vec()
            }
            _ => panic!("unexpected command: {:?}", spec.effective_argv()),
        };
        Ok(Captured {
            code,
            stdout,
            stderr: Vec::new(),
        })
    }
}

#[test]
fn export_preserves_exact_helper_argv_cwd_receipt_and_summary() {
    let fixture = Fixture::new();
    let mut runner = FakeRunner::new(&fixture, Effect::None);
    let mut calls = 0;
    let summary = export_with(&fixture.options, &fixture.runtime, &mut runner, &mut || {
        calls += 1;
        Ok(CPU.into())
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(runner.calls.len(), 3);
    assert_eq!(
        runner.calls[0].effective_argv(),
        vec!["dpkg-query", "-W", "-f=${Version}", "libllvm18"]
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
    );
    let root = fixture.options.root.canonicalize().unwrap();
    let expected = vec![
        fixture.options.python.clone().into_os_string(),
        "-P".into(),
        root.join(EXPORTER_SOURCES[0]).into_os_string(),
        "--trusted-model-directory".into(),
        fixture.options.fixture.join("models").into_os_string(),
        "--backend".into(),
        "cpu".into(),
        "--output".into(),
        fixture.options.catalog.clone().into_os_string(),
    ];
    assert_eq!(runner.calls[1].effective_argv(), expected);
    assert_eq!(runner.calls[1].cwd.as_deref(), Some(root.as_path()));
    assert!(runner.calls[1].inherit_output);
    assert!(runner.calls[1].env.is_empty());
    assert!(runner.calls[1].env_remove.is_empty());
    assert_eq!(
        runner.calls[2].effective_argv(),
        vec![
            OsString::from("git"),
            "-C".into(),
            root.into_os_string(),
            "rev-parse".into(),
            "HEAD".into()
        ]
    );
    let receipt: Value =
        serde_json::from_slice(&fs::read(&fixture.options.receipt).unwrap()).unwrap();
    assert_eq!(receipt["provider"], "CPU:LLVM");
    assert_eq!(receipt["version"], 1);
    assert_eq!(
        receipt["source_commit"],
        "0123456789012345678901234567890123456789"
    );
    assert_eq!(
        receipt["historical_fixture_manifest_sha256"],
        fixture.options.fixture_sha256
    );
    assert_eq!(
        receipt["lock"],
        pin(&fixture.options.root.join("uv.lock")).unwrap()
    );
    assert_eq!(
        receipt["llvm"]["library"],
        pin(&fixture.runtime.llvm).unwrap()
    );
    assert_eq!(receipt["models"].as_object().unwrap().len(), 5);
    assert_eq!(receipt["exporter_sources"].as_object().unwrap().len(), 14);
    assert_eq!(receipt["historical_catalog_generation"], "historical");
    assert_eq!(receipt["llvm"]["package_version"], "18.1.8-1\n");
    assert_eq!(receipt["host_cpu"], cpu_identity(CPU).unwrap());
    assert_eq!(receipt["catalog_files"].as_object().unwrap().len(), 2);
    for name in MODELS {
        assert_eq!(
            receipt["models"][name],
            pin(&fixture.options.fixture.join("models").join(name)).unwrap()
        );
    }
    for name in EXPORTER_SOURCES {
        assert_eq!(
            receipt["exporter_sources"][name],
            pin(&fixture.options.root.join(name)).unwrap()
        );
    }
    assert_eq!(summary["model_compilation_repeated"], false);
    assert_eq!(
        summary["receipt_pin"],
        pin(&fixture.options.receipt).unwrap()
    );
}

#[test]
fn failed_package_query_never_invokes_exporter_or_writes_destinations() {
    let fixture = Fixture::new();
    let mut runner = FakeRunner::new(&fixture, Effect::PackageQueryFailure);
    let mut cpu_called = false;
    assert!(
        export_with(&fixture.options, &fixture.runtime, &mut runner, &mut || {
            cpu_called = true;
            Ok(CPU.into())
        })
        .is_err()
    );
    assert!(!cpu_called);
    assert_eq!(runner.calls.len(), 1);
    assert!(!fixture.options.catalog.exists());
    assert!(!fixture.options.receipt.exists());
}

#[test]
fn post_helper_mutations_and_failures_keep_partial_catalog_but_no_receipt() {
    for effect in [
        Effect::Model,
        Effect::Source,
        Effect::Lock,
        Effect::Llvm,
        Effect::CatalogSymlink,
        Effect::BadGeneration,
        Effect::HelperFailure,
        Effect::GitFailure,
    ] {
        let fixture = Fixture::new();
        let mut runner = FakeRunner::new(&fixture, effect);
        assert!(
            export_with(&fixture.options, &fixture.runtime, &mut runner, &mut || Ok(
                CPU.into()
            ))
            .is_err()
        );
        assert!(fixture.options.catalog.exists());
        assert!(!fixture.options.receipt.exists());
        assert_eq!(
            runner.calls.len(),
            if matches!(effect, Effect::GitFailure) {
                3
            } else {
                2
            }
        );
    }
}

#[test]
fn cpu_drift_and_create_new_receipt_collision_fail_without_overwrite() {
    let fixture = Fixture::new();
    let mut runner = FakeRunner::new(&fixture, Effect::None);
    let mut calls = 0;
    assert!(
        export_with(&fixture.options, &fixture.runtime, &mut runner, &mut || {
            calls += 1;
            Ok(if calls == 1 {
                CPU.into()
            } else {
                "model name: changed\nflags: a\n".into()
            })
        })
        .is_err()
    );
    assert_eq!(runner.calls.len(), 2);
    assert!(!fixture.options.receipt.exists());
    let fixture = Fixture::new();
    let mut runner = FakeRunner::new(&fixture, Effect::ReceiptCollision);
    assert!(
        export_with(&fixture.options, &fixture.runtime, &mut runner, &mut || Ok(
            CPU.into()
        ))
        .is_err()
    );
    assert_eq!(runner.calls.len(), 3);
    assert_eq!(
        fs::read(&fixture.options.receipt).unwrap(),
        b"keep concurrent receipt"
    );
}

#[test]
fn preflight_bad_pins_provider_and_cached_mode_never_start_helper_or_create_outputs() {
    for mode in 0..4 {
        let mut fixture = Fixture::new();
        match mode {
            0 => fixture.options.fixture_sha256 = "wrong".into(),
            1 => fs::write(
                fixture.options.fixture.join("models").join(MODELS[0]),
                b"changed",
            )
            .unwrap(),
            2 => fixture.runtime.dev = "CPU".into(),
            _ => fixture.runtime.cached_code_configured = true,
        }
        let mut runner = FakeRunner::new(&fixture, Effect::None);
        assert!(
            export_with(&fixture.options, &fixture.runtime, &mut runner, &mut || Ok(
                CPU.into()
            ))
            .is_err()
        );
        assert!(runner.calls.is_empty());
        assert!(!fixture.options.catalog.exists());
        assert!(!fixture.options.receipt.exists());
    }
}

#[test]
fn cli_requires_original_five_options_and_explicit_current_interpreter_binding() {
    let args = [
        "--root",
        "/r",
        "--fixture",
        "/f",
        "--catalog",
        "/c",
        "--receipt",
        "/o",
        "--fixture-sha256",
        "sha",
        "--python",
        "/venv/bin/python",
    ]
    .map(OsString::from);
    let options = parse_args(&args).unwrap().unwrap();
    assert_eq!(options.python, PathBuf::from("/venv/bin/python"));
    assert!(parse_args(&args[..10]).is_err());
    assert!(parse_args(&["--help".into()]).unwrap().is_none());
}
