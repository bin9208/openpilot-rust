use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_adapter_sanitizers::{check, Kind},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};
fn bytes(value: &Value) -> Vec<u8> {
    value
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn physical(root: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.into()
    } else {
        root.join(path)
    }
}
struct Peer<'a> {
    root: &'a Path,
    row: &'a Value,
    calls: Vec<Value>,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert_eq!(spec.cwd.as_deref(), Some(self.root));
        assert!(spec.inherit_stdin);
        assert!(spec.merge_output);
        let n = self.calls.len();
        self.calls.push(json!({"argv":spec.argv,"env":spec.env}));
        let name = self.row["name"].as_str().unwrap();
        if name == "spawn" {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
        }
        let mut stdout = format!("out-{n}").into_bytes();
        stdout.extend(b"\xff\r\n");
        let code = if name == format!("failure-{n}") || name == "old-result-failure" {
            7
        } else {
            0
        };
        if code == 0 {
            let out = physical(self.root, spec.argv.last().unwrap());
            if n == 0 {
                fs::write(out, b"archive")?;
            } else if n == 1 {
                fs::write(out, b"program")?;
            } else if name != "missing-capture" {
                fs::write(out, b"png-fixture")?;
            }
        }
        Ok(Captured {
            code,
            stdout,
            stderr: vec![],
        })
    }
}
#[test]
fn original_model_and_training_commands_and_failure_artifacts_match() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/ui-adapter-sanitizers/original.json")).unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let target = root.join("target");
        let evidence = root.join("evidence");
        let raylib = root.join("raylib");
        let name = row["name"].as_str().unwrap();
        for tag in ["a", "b"] {
            for (skip,p) in [(name=="missing-header",target.join(format!("build/openpilot-startup-ui-{tag}/out/cxxbridge/include/openpilot-startup-ui/src/bridge.rs.h"))),(name=="missing-cxx",target.join(format!("deps/libcxx-{tag}.rlib")))]{if skip{continue;}fs::create_dir_all(p.parent().unwrap()).unwrap();fs::write(&p,tag).unwrap();fs::File::open(p).unwrap().set_modified(UNIX_EPOCH+Duration::from_secs(if name=="mtime-tie"{100}else if tag=="b"{300}else{200})).unwrap();}
        }
        if ["preexisting", "old-result-failure"].contains(&name) {
            fs::create_dir(&evidence).unwrap();
            fs::write(evidence.join("results.json"), b"old-result").unwrap();
            fs::write(evidence.join("untouched"), b"keep").unwrap();
        }
        let relative = name == "relative";
        let mut peer = Peer {
            root,
            row,
            calls: vec![],
        };
        let mut guards = 0;
        let result = check(
            root,
            if relative {
                Path::new("target")
            } else {
                &target
            },
            if relative {
                Path::new("raylib")
            } else {
                &raylib
            },
            if relative {
                Path::new("evidence")
            } else {
                &evidence
            },
            if row["kind"] == "model" {
                Kind::Model
            } else {
                Kind::Training
            },
            &mut peer,
            &mut |_| {
                guards += 1;
                Ok(())
            },
        );
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "{} {}: {result:?}",
            row["kind"],
            row["name"]
        );
        let mut wanted = row["calls"].as_array().unwrap().clone();
        if name == "mtime-tie" && !wanted.is_empty() {
            let header = fs::read_dir(target.join("build"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path()
                .join("out/cxxbridge/include");
            let cxx = fs::read_dir(target.join("deps"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            for call in &mut wanted {
                for a in call["argv"].as_array_mut().unwrap() {
                    let v = a.as_str().unwrap();
                    if v.starts_with("cxx=") {
                        *a = json!(format!("cxx={}", cxx.display())
                            .replace(&root.display().to_string(), "ROOT"));
                    } else if v.starts_with("-IROOT/target/build/") {
                        *a = json!(format!("-I{}", header.display())
                            .replace(&root.display().to_string(), "ROOT"));
                    }
                }
            }
        }
        for call in &mut wanted {
            call.as_object_mut().unwrap().remove("log");
        }
        let observed = serde_json::to_string(&peer.calls)
            .unwrap()
            .replace(&root.display().to_string(), "ROOT");
        assert_eq!(
            serde_json::from_str::<Value>(&observed).unwrap(),
            json!(wanted),
            "{} {}",
            row["kind"],
            row["name"]
        );
        assert_eq!(guards, peer.calls.len().min(2));
        let mut actual = BTreeMap::new();
        if evidence.is_dir() {
            for e in fs::read_dir(&evidence).unwrap() {
                let p = e.unwrap().path();
                let mut data = fs::read(&p).unwrap();
                if p.file_name().unwrap() == "results.json" {
                    data = String::from_utf8(data)
                        .unwrap()
                        .replace(&root.display().to_string(), "ROOT")
                        .into_bytes();
                }
                actual.insert(p.file_name().unwrap().to_string_lossy().into_owned(), data);
            }
        }
        let mut expected = row["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), bytes(v)))
            .collect::<BTreeMap<_, _>>();
        if name == "mtime-tie" && expected.contains_key("results.json") {
            let mut report: Value = serde_json::from_slice(&expected["results.json"]).unwrap();
            report["invocations"] =
                json!(wanted.iter().map(|c| c["argv"].clone()).collect::<Vec<_>>());
            let actual_report: Value = serde_json::from_slice(&actual["results.json"]).unwrap();
            assert_eq!(actual_report, report);
            expected.remove("results.json");
            actual.remove("results.json");
        }
        assert_eq!(actual, expected, "{} {}", row["kind"], row["name"]);
    }
}
fn raw_path(hex: &Value) -> PathBuf {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    OsString::from_vec(bytes(hex)).into()
}
fn replace_bytes(input: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    let mut rest = input;
    let mut output = vec![];
    while let Some(at) = rest.windows(old.len()).position(|v| v == old) {
        output.extend_from_slice(&rest[..at]);
        output.extend_from_slice(new);
        rest = &rest[at + old.len()..];
    }
    output.extend_from_slice(rest);
    output
}
struct RawPeer<'a> {
    base: &'a Path,
    calls: Vec<Vec<Vec<u8>>>,
}
impl Runner for RawPeer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        use std::os::unix::ffi::OsStrExt;
        let argv = spec.effective_argv();
        self.calls.push(
            argv.iter()
                .map(|arg| replace_bytes(arg.as_bytes(), self.base.as_os_str().as_bytes(), b"BASE"))
                .collect(),
        );
        fs::write(
            Path::new(argv.last().unwrap()),
            if self.calls.len() == 3 {
                b"png-fixture".as_slice()
            } else {
                b"product".as_slice()
            },
        )?;
        Ok(Captured {
            code: 0,
            stdout: b"peer\xff\r\n".to_vec(),
            stderr: vec![],
        })
    }
}
fn raw_layout(base: &Path, row: &Value) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let root = base.join(raw_path(&row["root"]));
    let target = root.join(raw_path(&row["target"]));
    let raylib = root.join(raw_path(&row["raylib"]));
    let evidence = root.join(raw_path(&row["evidence"]));
    let header = target
        .join("build")
        .join(raw_path(&row["header"]))
        .join("out/cxxbridge/include/openpilot-startup-ui/src/bridge.rs.h");
    fs::create_dir_all(header.parent().unwrap()).unwrap();
    fs::write(header, b"header").unwrap();
    fs::create_dir_all(target.join("deps")).unwrap();
    fs::write(target.join("deps").join(raw_path(&row["cxx"])), b"rlib").unwrap();
    (root, target, raylib, evidence)
}
#[test]
fn twelve_original_nonunicode_path_axes_preserve_commands_and_receipt_bytes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/ui-adapter-sanitizers/nonunicode.json"
    ))
    .unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let (root, target, raylib, evidence) = raw_layout(temp.path(), row);
        let mut peer = RawPeer {
            base: temp.path(),
            calls: vec![],
        };
        check(
            &root,
            &target,
            &raylib,
            &evidence,
            if row["kind"] == "model" {
                Kind::Model
            } else {
                Kind::Training
            },
            &mut peer,
            &mut |_| Ok(()),
        )
        .unwrap();
        let wanted = row["commands_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|command| {
                command
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(bytes)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(peer.calls, wanted, "{row}");
        assert_eq!(
            fs::read(evidence.join("results.json")).unwrap(),
            row["receipt"]
                .as_str()
                .unwrap()
                .replace("BASE", temp.path().to_str().unwrap())
                .as_bytes(),
            "{row}"
        );
    }
}
#[test]
fn actual_cli_accepts_nonunicode_root_and_preserves_raw_child_arguments() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/ui-adapter-sanitizers/nonunicode.json"
    ))
    .unwrap();
    let row = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "model" && row["axis"] == "root")
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (root, target, raylib, evidence) = raw_layout(temp.path(), row);
    let peers = temp.path().join("peers");
    fs::create_dir(&peers).unwrap();
    let calls = temp.path().join("calls");
    let peer = r#"#!/bin/sh
printf '%s\0' "$@" >> "$RAW_ARGV_CAPTURE"
while [ "$#" -gt 0 ]; do if [ "$1" = -o ]; then shift; out=$1; break; fi; shift; done
case "$0" in
*/rustup) printf product > "$out";;
*) cat > "$out" <<'PROBE'
#!/bin/sh
printf '%s\0' "$@" >> "$RAW_ARGV_CAPTURE"
for last; do :; done
printf png-fixture > "$last"
PROBE
chmod +x "$out";;
esac
"#;
    for name in ["rustup", "clang++"] {
        let p = peers.join(name);
        fs::write(&p, peer).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_check-ui-model-adapter"))
        .arg("--target")
        .arg(&target)
        .arg("--raylib")
        .arg(&raylib)
        .arg("--evidence")
        .arg(&evidence)
        .current_dir(&root)
        .env(
            "PATH",
            format!("{}:{}", peers.display(), std::env::var("PATH").unwrap()),
        )
        .env("RAW_ARGV_CAPTURE", &calls)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = fs::read(calls).unwrap();
    let wanted =
        row["commands_hex"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|command| {
                command.as_array().unwrap().iter().skip(1).map(|arg| {
                    replace_bytes(&bytes(arg), b"BASE", temp.path().as_os_str().as_bytes())
                })
            })
            .flat_map(|mut arg| {
                arg.push(0);
                arg
            })
            .collect::<Vec<_>>();
    assert_eq!(raw, wanted);
    assert_eq!(
        fs::read(evidence.join("results.json")).unwrap(),
        row["receipt"]
            .as_str()
            .unwrap()
            .replace("BASE", temp.path().to_str().unwrap())
            .as_bytes()
    );
}

#[test]
fn capacity_failure_stops_before_second_compiler_and_never_writes_success() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let target = root.join("target");
    let evidence = root.join("evidence");
    let header=target.join("build/openpilot-startup-ui-fixture/out/cxxbridge/include/openpilot-startup-ui/src/bridge.rs.h");
    fs::create_dir_all(header.parent().unwrap()).unwrap();
    fs::write(header, b"header").unwrap();
    fs::create_dir_all(target.join("deps")).unwrap();
    fs::write(target.join("deps/libcxx-fixture.rlib"), b"library").unwrap();
    let row = json!({"name":"success"});
    let mut peer = Peer {
        root,
        row: &row,
        calls: vec![],
    };
    let mut guards = 0;
    let result = check(
        root,
        &target,
        &root.join("raylib"),
        &evidence,
        Kind::Model,
        &mut peer,
        &mut |path| {
            assert_eq!(path, evidence);
            guards += 1;
            if guards == 2 {
                Err("owned capacity failure".into())
            } else {
                Ok(())
            }
        },
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("owned capacity failure"));
    assert_eq!(peer.calls.len(), 1);
    assert_eq!(guards, 2);
    assert_eq!(fs::read(evidence.join("0.log")).unwrap(), b"out-0\xff\r\n");
    assert_eq!(
        fs::read(evidence.join("libcxx_runtime.a")).unwrap(),
        b"archive"
    );
    for name in ["1.log", "model-adapter-asan", "results.json"] {
        assert!(!evidence.join(name).exists());
    }
}
#[test]
fn eighteen_original_glob_admissions_preserve_non_directories_and_dangling_failures() {
    use std::os::unix::fs::symlink;
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/ui-adapter-sanitizers/glob.json")).unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let target = root.join("target");
        let evidence = root.join("evidence");
        let raylib = root.join("raylib");
        let valid=target.join("build/openpilot-startup-ui-valid/out/cxxbridge/include/openpilot-startup-ui/src/bridge.rs.h");
        fs::create_dir_all(valid.parent().unwrap()).unwrap();
        fs::write(&valid, b"valid").unwrap();
        fs::create_dir_all(target.join("deps")).unwrap();
        fs::write(target.join("deps/libcxx-valid.rlib"), b"rlib").unwrap();
        let prefix = target.join("build/openpilot-startup-ui-nope");
        let extra = prefix.join("out/cxxbridge/include/openpilot-startup-ui/src/bridge.rs.h");
        match row["case"].as_str().unwrap() {
            "prefix-file" => fs::write(prefix, b"not directory").unwrap(),
            "prefix-dangling" => symlink(root.join("missing"), prefix).unwrap(),
            "prefix-file-symlink" => {
                fs::write(root.join("file"), b"file").unwrap();
                symlink(root.join("file"), prefix).unwrap();
            }
            "out-file" => {
                fs::create_dir(&prefix).unwrap();
                fs::write(prefix.join("out"), b"file").unwrap();
            }
            "header-dangling" => {
                fs::create_dir_all(extra.parent().unwrap()).unwrap();
                symlink(root.join("missing"), &extra).unwrap();
            }
            "header-valid-symlink" => {
                fs::create_dir_all(extra.parent().unwrap()).unwrap();
                symlink(&valid, &extra).unwrap();
            }
            "header-directory" => {
                fs::create_dir_all(&extra).unwrap();
            }
            "cxx-dangling" => {
                symlink(root.join("missing"), target.join("deps/libcxx-nope.rlib")).unwrap();
            }
            "cxx-directory" => {
                fs::create_dir(target.join("deps/libcxx-nope.rlib")).unwrap();
            }
            _ => unreachable!(),
        }
        fs::File::open(&valid)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(100))
            .unwrap();
        fs::File::open(target.join("deps/libcxx-valid.rlib"))
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(100))
            .unwrap();
        if row["case"] == "header-directory" {
            fs::File::open(&extra)
                .unwrap()
                .set_modified(UNIX_EPOCH + Duration::from_secs(200))
                .unwrap();
        }
        if row["case"] == "cxx-directory" {
            fs::File::open(target.join("deps/libcxx-nope.rlib"))
                .unwrap()
                .set_modified(UNIX_EPOCH + Duration::from_secs(200))
                .unwrap();
        }
        let mut row = row.clone();
        if row["case"] == "header-valid-symlink" {
            // Both aliases stat the same target. Python max keeps the first
            // candidate in the live unsorted glob order when timestamps tie.
            let first = fs::read_dir(target.join("build"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path()
                .join("out/cxxbridge/include");
            let actual = format!("-I{}", first.display()).replace(root.to_str().unwrap(), "ROOT");
            let captured = row["calls"][1][5].as_str().unwrap().to_owned();
            row["calls"][1][5] = Value::String(actual.clone());
            row["receipt"] =
                Value::String(row["receipt"].as_str().unwrap().replace(&captured, &actual));
        }
        let mut peer = RawPeer {
            base: root,
            calls: vec![],
        };
        let result = check(
            root,
            &target,
            &raylib,
            &evidence,
            if row["kind"] == "model" {
                Kind::Model
            } else {
                Kind::Training
            },
            &mut peer,
            &mut |_| Ok(()),
        );
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "{} {}: {result:?}",
            row["kind"],
            row["case"]
        );
        let wanted = row["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|command| {
                command
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|arg| arg.as_str().unwrap().replace("ROOT", "BASE").into_bytes())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(peer.calls, wanted, "{} {}", row["kind"], row["case"]);
        if row["receipt"].is_null() {
            assert!(!evidence.join("results.json").exists());
        } else {
            assert_eq!(
                fs::read(evidence.join("results.json")).unwrap(),
                row["receipt"]
                    .as_str()
                    .unwrap()
                    .replace("ROOT", root.to_str().unwrap())
                    .as_bytes(),
                "{} {}",
                row["kind"],
                row["case"]
            );
        }
    }
}
