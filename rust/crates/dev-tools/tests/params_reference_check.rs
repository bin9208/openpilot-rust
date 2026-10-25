use openpilot_dev_tools::{
    params_reference_check::{check, ParamsRunner},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
struct Peer {
    mode: String,
    calls: Vec<Value>,
    root: PathBuf,
    temporary: Option<PathBuf>,
}
impl Drop for Peer {
    fn drop(&mut self) {
        if let Some(path) = &self.temporary {
            let _ = openpilot_dev_tools::params_reference_check::cleanup_temporary(path);
        }
    }
}
impl Runner for Peer {
    fn run(&mut self, s: &CommandSpec) -> Result<Captured> {
        assert_eq!(s.timeout, Duration::MAX);
        let argv = s.effective_argv();
        let a = argv
            .iter()
            .map(|x| x.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let (mut code, mut stdout) = (0, vec![]);
        if a[0] == "capnp" {
            assert!(s.inherit_output && s.inherit_stdin);
            assert!(s.input.is_none());
            let schema = PathBuf::from(a[2].strip_prefix("-I").unwrap());
            let temp = schema.parent().unwrap().to_path_buf();
            self.temporary = Some(temp.clone());
            for name in ["log", "custom", "deprecated"] {
                assert_eq!(
                    fs::read(schema.join(format!("{name}.capnp")))?,
                    fs::read(self.root.join(format!("openpilot/cereal/{name}.capnp")))?
                );
            }
            assert_eq!(
                fs::read(schema.join("car.capnp"))?,
                fs::read(self.root.join("opendbc_repo/opendbc/car/car.capnp"))?
            );
            assert!(schema.join("include").is_dir());
            let mut names = a[5..]
                .iter()
                .map(|p| Path::new(p).file_name().unwrap().to_str().unwrap())
                .collect::<Vec<_>>();
            names.sort();
            assert_eq!(
                names,
                ["car.capnp", "custom.capnp", "deprecated.capnp", "log.capnp"]
            );
            if self.mode.starts_with("readonly-") {
                use std::os::unix::fs::PermissionsExt;
                let d = temp.join("readonly");
                fs::create_dir(&d)?;
                fs::write(d.join("value"), b"owned")?;
                fs::set_permissions(&d, fs::Permissions::from_mode(0o500))?;
            }
            if self.mode == "capnp-fail" || self.mode == "readonly-failure" {
                code = 7;
            }
        } else if a[0] == "g++" {
            assert!(s.inherit_output && s.inherit_stdin);
            if self.mode == "compiler-fail" {
                code = 7;
            } else {
                fs::write(a.last().unwrap(), b"native")?;
            }
        } else {
            assert!(s.inherit_stderr);
            assert!(!s.inherit_output);
            assert!(!s.inherit_stdin);
            assert_eq!(a.len(), 5);
            assert_eq!(a[2], "d");
            let input = s.input.as_ref().unwrap();
            let d = Path::new(&a[1]).join("d");
            fs::create_dir_all(&d)?;
            fs::write(Path::new(&a[1]).join(".lock"), b"")?;
            let (op, key) = (a[3].as_str(), a[4].as_str());
            if self.mode == format!("{op}-fail") {
                code = 7;
            } else {
                match op {
                    "catalog" => {
                        stdout = if self.mode == "catalog-mismatch"
                            && !a[0].ends_with("params-reference")
                        {
                            b"A\n".to_vec()
                        } else {
                            b"A\nB\n".to_vec()
                        }
                    }
                    "put" => {
                        if self.mode == "remove-loop" && d.join(key).is_symlink() {
                            fs::remove_file(d.join(key))?;
                        }
                        fs::write(
                            d.join(key),
                            if self.mode == "empty-on-disk" && input.is_empty() {
                                b"wrong".as_slice()
                            } else {
                                input.as_slice()
                            },
                        )?;
                    }
                    "get" => {
                        stdout = fs::read(d.join(key)).unwrap_or_default();
                        if (self.mode == "binary-mismatch" && stdout.len() > 1000)
                            || (self.mode == "empty-return" && stdout.is_empty())
                            || (self.mode == "persistent-mismatch" && key == "IsMetric")
                        {
                            stdout = b"wrong".to_vec();
                        }
                    }
                    "remove" => {
                        if self.mode != "remove-retained" {
                            let _ = fs::remove_file(d.join(key));
                            if self.mode == "remove-loop" {
                                std::os::unix::fs::symlink(key, d.join(key))?;
                            }
                        }
                    }
                    "clear" => {
                        for name in ["CarParams", "UnknownOldKey"] {
                            if self.mode
                                != if name == "CarParams" {
                                    "clear-transient-retained"
                                } else {
                                    "clear-unknown-retained"
                                }
                            {
                                let _ = fs::remove_file(d.join(name));
                            }
                        }
                    }
                    _ => panic!("unexpected operation"),
                }
            }
        }
        let normalize = |v: &str| {
            v.replace(self.temporary.as_ref().unwrap().to_str().unwrap(), "TEMP")
                .replace(self.root.to_str().unwrap(), "ROOT")
        };
        self.calls.push(serde_json::json!({"argv":a.iter().map(|v|normalize(v)).collect::<Vec<_>>(),"input_bytes":s.input.as_ref().map_or(0,Vec::len),"input_sha256":({use sha2::{Digest,Sha256};format!("{:x}",Sha256::digest(s.input.as_deref().unwrap_or_default()))}),"stdout_pipe":!s.inherit_output,"check":true}));
        Ok(Captured {
            code,
            stdout,
            stderr: vec![],
        })
    }
}
impl ParamsRunner for Peer {
    fn cleanup_temporary(&mut self, path: &Path) -> Result<()> {
        if self.mode == "cleanup-fail" {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "controlled cleanup failure",
            )
            .into())
        } else {
            openpilot_dev_tools::params_reference_check::cleanup_temporary(path)
        }
    }
    fn locked_put(&mut self, writer: &Path, store: &Path) -> Result<()> {
        let normalize = |p: &Path| {
            p.to_string_lossy()
                .replace(self.temporary.as_ref().unwrap().to_str().unwrap(), "TEMP")
                .replace(self.root.to_str().unwrap(), "ROOT")
        };
        self.calls.push(
            serde_json::json!({"popen":[normalize(writer),normalize(store),"d","put","CarParams"]}),
        );
        self.calls.push(serde_json::json!({"wait":0.1}));
        if self.mode != "lock-unblocked" {
            self.calls.push(serde_json::json!({"wait":10}));
        }
        self.calls
            .push(serde_json::json!({"child_context_exit":true}));
        if ["lock-unblocked", "lock-exit", "lock-timeout"].contains(&self.mode.as_str()) {
            return Err("controlled lock failure".into());
        }
        fs::write(
            store.join("d/CarParams"),
            if self.mode == "lock-content" {
                b"wrong".as_slice()
            } else {
                b"lock parity".as_slice()
            },
        )?;
        Ok(())
    }
}
fn captured_cases(selected: Option<&str>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let original: Value =
        serde_json::from_str(include_str!("fixtures/params-reference/original.json")).unwrap();
    for row in original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| selected.is_none_or(|name| r["mode"] == name))
    {
        if row["mode"] == "second-catalog-mismatch" {
            continue;
        }
        let mut peer = Peer {
            mode: row["mode"].as_str().unwrap().into(),
            calls: vec![],
            root: root.clone(),
            temporary: None,
        };
        let candidate = root.join(".analysis/scratch/2026-10-16-params-reference/candidate");
        let prefix = root.join(".analysis/scratch/2026-10-16-params-reference/capnp-prefix");
        let mut printed = vec![];
        let result = check(
            &root,
            &candidate,
            if peer.mode == "pass-prefix" {
                Some(prefix.as_path())
            } else {
                None
            },
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_ok(),
            row["ok"] == true,
            "{} {result:?}",
            row["mode"]
        );
        assert_eq!(String::from_utf8(printed).unwrap(), row["stdout"]);
        let mut expected = row["calls"].as_array().unwrap().clone();
        if let Some(first) = expected.first_mut() {
            if first.get("argv").is_some() {
                let actual = peer.calls[0]["argv"].as_array().unwrap();
                let target = first["argv"].as_array_mut().unwrap();
                let mut a = actual[5..].to_vec();
                let mut b = target[5..].to_vec();
                a.sort_by_key(|v| v.to_string());
                b.sort_by_key(|v| v.to_string());
                assert_eq!(a, b);
                target[5..].clone_from_slice(&actual[5..]);
            }
        }
        assert_eq!(peer.calls, expected, "{}", row["mode"]);
        let temporary = peer.temporary.as_ref().unwrap();
        assert_eq!(
            !temporary.exists(),
            row["temporary_removed"].as_bool().unwrap(),
            "temporary cleanup"
        );
        if temporary.exists() {
            openpilot_dev_tools::params_reference_check::cleanup_temporary(temporary).unwrap();
        }
    }
}

#[test]
fn original_compilation_interoperability_assertions_lock_phases_and_cleanup() {
    captured_cases(None)
}
#[test]
fn original_exists_ignores_self_symlink_loop() {
    captured_cases(Some("remove-loop"))
}
#[test]
fn real_owned_child_respects_lock_and_failed_children_are_reaped() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let helper = temp.path().join("lock-peer");
    openpilot_disk_space::inspect(temp.path(), 64 * 1024 * 1024)
        .unwrap()
        .require()
        .unwrap();
    let compiled = std::process::Command::new("g++")
        .arg("-std=c++17")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/params-reference/lock-peer.cc"),
        )
        .arg("-o")
        .arg(&helper)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    for mode in ["pass", "unblocked", "exit", "hang"] {
        let root = temp.path().join(mode);
        fs::create_dir_all(root.join("d")).unwrap();
        fs::write(root.join(".lock"), b"").unwrap();
        let writer = temp.path().join(format!("writer-{mode}"));
        fs::write(
            &writer,
            format!(
                "#!/bin/sh\nPARAMS_LOCK_PEER_MODE={mode} exec '{}' \"$@\"\n",
                helper.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&writer, fs::Permissions::from_mode(0o755)).unwrap();
        let start = std::time::Instant::now();
        let result = openpilot_dev_tools::params_reference_check::locked_put(&writer, &root);
        assert_eq!(result.is_ok(), mode == "pass", "{mode}: {result:?}");
        assert!(
            start.elapsed() < Duration::from_secs(12),
            "owned cleanup exceeded deadline"
        );
        if mode == "pass" {
            assert!(start.elapsed() >= Duration::from_millis(100));
            assert_eq!(fs::read(root.join("d/CarParams")).unwrap(), b"lock parity");
        }
        let pid = fs::read_to_string(root.join("pid")).unwrap();
        assert!(
            !Path::new("/proc").join(pid.trim()).exists(),
            "owned child not reaped"
        );
    }
}

#[test]
fn original_schema_copy_failures_keep_phase_and_partial_tree() {
    use openpilot_dev_tools::params_reference_check::build_native;
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("compiler must not run before failed copy")
        }
    }
    impl ParamsRunner for Never {
        fn locked_put(&mut self, _: &Path, _: &Path) -> Result<()> {
            panic!("unexpected lock")
        }
    }
    let original: Value =
        serde_json::from_str(include_str!("fixtures/params-reference/copy-original.json")).unwrap();
    for row in original["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let out = temp.path().join("out");
        fs::create_dir(&out).unwrap();
        fs::create_dir_all(root.join("openpilot/cereal/include")).unwrap();
        fs::create_dir_all(root.join("opendbc_repo/opendbc/car")).unwrap();
        for name in ["log", "custom", "deprecated"] {
            fs::write(root.join(format!("openpilot/cereal/{name}.capnp")), name).unwrap();
        }
        fs::write(root.join("opendbc_repo/opendbc/car/car.capnp"), b"car").unwrap();
        fs::write(root.join("openpilot/cereal/include/okay.capnp"), b"include").unwrap();
        match row["mode"].as_str().unwrap() {
            "read-error" => {
                let p = root.join("openpilot/cereal/log.capnp");
                fs::remove_file(&p).unwrap();
                std::os::unix::fs::symlink("/proc/self/mem", p).unwrap();
            }
            "fifo-schema" => {
                let p = root.join("openpilot/cereal/log.capnp");
                fs::remove_file(&p).unwrap();
                rustix::fs::mkfifoat(rustix::fs::CWD, &p, rustix::fs::Mode::from_raw_mode(0o600))
                    .unwrap();
            }
            "fifo-include" => rustix::fs::mkfifoat(
                rustix::fs::CWD,
                root.join("openpilot/cereal/include/pipe"),
                rustix::fs::Mode::from_raw_mode(0o600),
            )
            .unwrap(),
            "missing-log" => fs::remove_file(root.join("openpilot/cereal/log.capnp")).unwrap(),
            "missing-car" => {
                fs::remove_file(root.join("opendbc_repo/opendbc/car/car.capnp")).unwrap()
            }
            "missing-include" => fs::remove_dir_all(root.join("openpilot/cereal/include")).unwrap(),
            "broken-include" => {
                std::os::unix::fs::symlink("missing", root.join("openpilot/cereal/include/broken"))
                    .unwrap()
            }
            "schema-exists" => fs::create_dir(out.join("schema")).unwrap(),
            "generated-exists" => fs::create_dir_all(out.join("cereal/gen/cpp")).unwrap(),
            "missing-output" => fs::remove_dir(&out).unwrap(),
            _ => unreachable!(),
        }
        assert!(
            build_native(&root, &out, None, &mut Never).is_err(),
            "{}",
            row["mode"]
        );
        fn collect(base: &Path, dir: &Path, entries: &mut serde_json::Map<String, Value>) {
            if !dir.exists() {
                return;
            }
            for e in fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                let meta = fs::symlink_metadata(&p).unwrap();
                let value = if meta.file_type().is_symlink() {
                    "symlink".into()
                } else if meta.is_dir() {
                    "dir".into()
                } else {
                    fs::read(&p)
                        .unwrap()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                };
                entries.insert(
                    p.strip_prefix(base).unwrap().to_str().unwrap().into(),
                    Value::String(value),
                );
                if meta.is_dir() {
                    collect(base, &p, entries);
                }
            }
        }
        let mut actual = serde_json::Map::new();
        collect(&out, &out, &mut actual);
        assert_eq!(
            Value::Object(actual),
            row["output_entries"],
            "{}",
            row["mode"]
        );
    }
}

#[test]
fn original_copyfile_creation_differs_from_copytree_metadata_and_symlinks() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, UNIX_EPOCH};
    struct BuildOnly;
    impl Runner for BuildOnly {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            Ok(Captured {
                code: 0,
                stdout: vec![],
                stderr: vec![],
            })
        }
    }
    impl ParamsRunner for BuildOnly {
        fn locked_put(&mut self, _: &Path, _: &Path) -> Result<()> {
            unreachable!()
        }
    }
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/params-reference/metadata-original.json"
    ))
    .unwrap();
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("root");
    let out = t.path().join("out");
    fs::create_dir(&out).unwrap();
    fs::create_dir_all(root.join("openpilot/cereal/include/child")).unwrap();
    fs::create_dir_all(root.join("opendbc_repo/opendbc/car")).unwrap();
    let old = UNIX_EPOCH + Duration::from_nanos(original["top_source_mtime_ns"].as_u64().unwrap());
    for n in ["log", "custom", "deprecated"] {
        let p = root.join(format!("openpilot/cereal/{n}.capnp"));
        fs::write(&p, n).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        fs::File::open(&p)
            .unwrap()
            .set_times(fs::FileTimes::new().set_accessed(old).set_modified(old))
            .unwrap();
    }
    let top = root.join("openpilot/cereal/log.capnp");
    rustix::fs::setxattr(
        &top,
        "user.params-reference",
        b"top",
        rustix::fs::XattrFlags::empty(),
    )
    .unwrap();
    fs::write(root.join("opendbc_repo/opendbc/car/car.capnp"), b"car").unwrap();
    let private = root.join("openpilot/cereal/include/private.capnp");
    fs::write(&private, b"private").unwrap();
    fs::set_permissions(&private, fs::Permissions::from_mode(0o600)).unwrap();
    fs::File::open(&private)
        .unwrap()
        .set_times(fs::FileTimes::new().set_accessed(old).set_modified(old))
        .unwrap();
    rustix::fs::setxattr(
        &private,
        "user.params-reference",
        b"include",
        rustix::fs::XattrFlags::empty(),
    )
    .unwrap();
    fs::write(root.join("openpilot/cereal/include/child/value"), b"child").unwrap();
    std::os::unix::fs::symlink(
        "private.capnp",
        private.parent().unwrap().join("alias.capnp"),
    )
    .unwrap();
    std::os::unix::fs::symlink("child", private.parent().unwrap().join("alias-dir")).unwrap();
    let write_only = std::env::var_os("PARAMS_CHILD_WRITE_ONLY").is_some();
    let mask = write_only.then(|| rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o444)));
    let built = openpilot_dev_tools::params_reference_check::build_native(
        &root,
        &out,
        Some(Path::new("relative-prefix")),
        &mut BuildOnly,
    );
    if let Some(mask) = mask {
        rustix::process::umask(mask);
    }
    built.unwrap();
    let control = out.join("default-mode");
    fs::write(&control, b"").unwrap();
    let copied = out.join("schema/log.capnp");
    assert_eq!(
        copied.metadata().unwrap().permissions().mode() & 0o777,
        if write_only {
            0o666 & !0o444
        } else {
            control.metadata().unwrap().permissions().mode() & 0o777
        }
    );
    assert_ne!(copied.metadata().unwrap().modified().unwrap(), old);
    let mut attr = [0; 32];
    assert!(rustix::fs::getxattr(&copied, "user.params-reference", &mut attr).is_err());
    let copied = out.join("schema/include/private.capnp");
    assert_eq!(
        copied.metadata().unwrap().permissions().mode() & 0o777,
        original["include_file_mode"].as_u64().unwrap() as u32
    );
    assert_eq!(copied.metadata().unwrap().modified().unwrap(), old);
    let len = rustix::fs::getxattr(&copied, "user.params-reference", &mut attr).unwrap();
    assert_eq!(&attr[..len], b"include");
    let alias = out.join("schema/include/alias.capnp");
    assert!(!alias.is_symlink());
    assert_eq!(fs::read(alias).unwrap(), b"private");
    let alias = out.join("schema/include/alias-dir");
    assert!(!alias.is_symlink());
    assert_eq!(fs::read(alias.join("value")).unwrap(), b"child");
}

#[test]
fn original_cli_help_and_argument_errors_do_not_create_temporary_artifacts() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/params-reference/original-cli.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-params-reference"))
            .args(
                row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .env("TMPDIR", temp.path())
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            row["exit"].as_i64().map(|n| n as i32),
            "{row}"
        );
        assert_eq!(
            !result.stdout.is_empty(),
            row["stdout_nonempty"].as_bool().unwrap(),
            "{row}"
        );
        assert_eq!(
            !result.stderr.is_empty(),
            row["stderr_nonempty"].as_bool().unwrap(),
            "{row}"
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

#[test]
fn original_path_based_copystat_accepts_write_only_new_targets() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "original_copyfile_creation_differs_from_copytree_metadata_and_symlinks",
            "--nocapture",
        ])
        .env("PARAMS_CHILD_WRITE_ONLY", "1")
        .env("TMPDIR", temp.path())
        .output()
        .unwrap();
    fn repair(path: &Path) {
        if fs::symlink_metadata(path).unwrap().is_dir() {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
            for e in fs::read_dir(path).unwrap() {
                repair(&e.unwrap().path());
            }
        }
    }
    repair(temp.path());
    assert!(
        result.status.success(),
        "original path-based utime accepts child-only umask0444: {result:?}"
    );
}

#[test]
fn original_temporary_directory_candidate_fallback_and_error_cleanup() {
    use std::os::unix::fs::PermissionsExt;
    let original: Value =
        serde_json::from_str(include_str!("fixtures/params-reference/temp-original.json")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for row in original.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let bins = base.join("bin");
        fs::create_dir(&bins).unwrap();
        let marker = base.join("marker");
        let script = bins.join("capnp");
        fs::write(
            &script,
            b"#!/bin/sh\nprintf '%s' \"$2\" > \"$PARAMS_MARKER\"\nexit 7\n",
        )
        .unwrap();
        fs::set_permissions(script, fs::Permissions::from_mode(0o755)).unwrap();
        let selected = base.join("selected");
        let ignored = base.join("ignored");
        fs::create_dir(&selected).unwrap();
        fs::create_dir(&ignored).unwrap();
        let selected_case = row["mode"] == "TEMP-fallback";
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-params-reference"))
            .arg("--binary")
            .arg(base.join("unused"))
            .env("OPENPILOT_RUST_ROOT", &root)
            .env("TMPDIR", base.join("missing"))
            .env(
                "TEMP",
                if selected_case {
                    selected.clone()
                } else {
                    base.join("also-missing")
                },
            )
            .env(
                "TMP",
                if selected_case {
                    ignored.clone()
                } else {
                    base.join("third-missing")
                },
            )
            .env("PATH", &bins)
            .env("PARAMS_MARKER", &marker)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(
            marker.exists(),
            "must reach compiler after Python tempdir fallback: {result:?}"
        );
        let value = fs::read_to_string(marker).unwrap();
        let schema = Path::new(value.strip_prefix("-I").unwrap());
        assert_eq!(
            schema.parent().unwrap().parent().unwrap(),
            if selected_case {
                selected.as_path()
            } else {
                Path::new("/tmp")
            }
        );
        assert!(!schema.parent().unwrap().exists());
        assert_eq!(fs::read_dir(selected).unwrap().count(), 0);
        assert_eq!(fs::read_dir(ignored).unwrap().count(), 0);
    }
}
