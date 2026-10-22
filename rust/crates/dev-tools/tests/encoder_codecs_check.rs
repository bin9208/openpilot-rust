//! Original complete/partial artifact contracts, captured before implementation.
#![cfg(feature = "camera-sensor-native")]
use openpilot_dev_tools::{
    encoder_codecs_check::{execute, Options},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
const ORIGINAL: &str = include_str!("fixtures/encoder-codecs-check/original.json");
struct Peer {
    scenario: String,
    base: PathBuf,
    commands: Vec<Value>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert_eq!(spec.timeout, std::time::Duration::MAX);
        assert!(spec.inherit_stdin && spec.inherit_stderr);
        assert!(spec.input.is_none() && !spec.merge_output);
        let env = spec.effective_env();
        self.commands
            .push(json!({"argv":spec.argv,"kind":"flags","env":{
                "PKG_CONFIG_LIBDIR":env[std::ffi::OsStr::new("PKG_CONFIG_LIBDIR")].to_str().unwrap(),
                "PKG_CONFIG_SYSROOT_DIR":env[std::ffi::OsStr::new("PKG_CONFIG_SYSROOT_DIR")].to_str().unwrap(),
                "LD_LIBRARY_PATH":env[std::ffi::OsStr::new("LD_LIBRARY_PATH")].to_str().unwrap(),
            }}));
        Ok(Captured {
            code: if self.scenario == "pkg-failure" { 7 } else { 0 },
            stdout: if self.scenario == "pkg-invalid-quote" {
                b"\"unterminated".to_vec()
            } else {
                b"-I\"/synthetic include\" -L/synthetic -lavcodec -lavformat -lavutil\n".to_vec()
            },
            stderr: vec![],
        })
    }
    fn run_to_file(&mut self, spec: &CommandSpec, log: &Path) -> Result<Captured> {
        assert_eq!(spec.timeout, std::time::Duration::from_secs(120));
        assert!(spec.inherit_stdin && spec.input.is_none());
        let env = spec.effective_env();
        self.commands
            .push(json!({"argv":spec.argv,"kind":"run","log":log,"env":{
                "PKG_CONFIG_LIBDIR":env[std::ffi::OsStr::new("PKG_CONFIG_LIBDIR")].to_str().unwrap(),
                "PKG_CONFIG_SYSROOT_DIR":env[std::ffi::OsStr::new("PKG_CONFIG_SYSROOT_DIR")].to_str().unwrap(),
                "LD_LIBRARY_PATH":env[std::ffi::OsStr::new("LD_LIBRARY_PATH")].to_str().unwrap(),
            }}));
        fs::write(log, b"original controlled child\n")?;
        let mut code = 0;
        if spec.argv[0] == "g++" {
            if self.scenario == "build-failure" {
                code = 9;
            } else {
                fs::write(
                    self.base.join("out/source-oracle"),
                    b"compiled-source-oracle",
                )?;
            }
        } else {
            let rust = spec.argv[0] == self.base.join("rust-probe").to_str().unwrap();
            let output = Path::new(&spec.argv[1]);
            fs::create_dir(output)?;
            if self.scenario
                == if rust {
                    "rust-failure"
                } else {
                    "source-failure"
                }
            {
                code = 11;
            } else {
                fs::write(
                    output.join("trace.tsv"),
                    if rust && self.scenario == "trace-mismatch" {
                        "changed\n"
                    } else {
                        "packet 1\n"
                    },
                )?;
                if self.scenario != "missing-packets" {
                    let name = if rust && self.scenario == "missing-packet-name" {
                        "packet-99.bin"
                    } else {
                        "packet-00.bin"
                    };
                    fs::write(
                        output.join(name),
                        if rust && self.scenario == "packet-mismatch" {
                            b"changed".as_slice()
                        } else {
                            &[0, 1, 2, 255]
                        },
                    )?;
                    if rust && self.scenario == "extra-packets" {
                        fs::write(output.join("packet-01.bin"), b"extra")?;
                    }
                }
            }
        }
        Ok(Captured {
            code,
            stdout: vec![],
            stderr: vec![],
        })
    }
}
fn normalize(v: &Value, base: &Path) -> Value {
    match v {
        Value::String(s) => json!(s.replace(base.to_str().unwrap(), "$BASE")),
        Value::Array(a) => Value::Array(a.iter().map(|v| normalize(v, base)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| {
                    (
                        k.replace(base.to_str().unwrap(), "$BASE"),
                        normalize(v, base),
                    )
                })
                .collect(),
        ),
        v => v.clone(),
    }
}
fn files(base: &Path, path: &Path, out: &mut BTreeMap<String, Value>) {
    if !path.exists() {
        return;
    }
    for entry in fs::read_dir(path).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            files(base, &p, out);
        } else {
            let bytes = fs::read(&p).unwrap();
            let value = match String::from_utf8(bytes.clone()) {
                Ok(s) => json!({"text":s}),
                Err(_) => {
                    json!({"hex":bytes.iter().map(|b|format!("{b:02x}")).collect::<String>()})
                }
            };
            out.insert(
                p.strip_prefix(base.join("out"))
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .into(),
                normalize(&value, base),
            );
        }
    }
}
// Original captures remain immutable. Only the withdrawn fixed-floor field
// changes; compare every other artifact byte and command as captured.
fn growth_only_files(original: &Value) -> Value {
    let mut files = original.clone();
    for (name, file) in files.as_object_mut().unwrap() {
        if name == "space.json" || (name.starts_with("build-") && name.ends_with("-space.json")) {
            let text = file["text"].as_str().unwrap();
            let parsed: Value = serde_json::from_str(text).unwrap();
            assert_eq!(parsed["floor"], 25u64 << 30);
            let changed = text.replace("26843545600", "0");
            let expected: Value = serde_json::from_str(&changed).unwrap();
            let mut without_floor = parsed;
            without_floor["floor"] = 0.into();
            assert_eq!(expected, without_floor);
            file["text"] = changed.into();
        }
    }
    files
}

#[test]
fn original_codec_main_preserves_all_commands_and_partial_artifact_bytes() {
    let original: Value = serde_json::from_str(ORIGINAL).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for case in original["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let base = temporary.path();
        let source = base.join("root");
        for path in [
            "rust/tools/encoder_oracle/adapter.h",
            "rust/tools/encoder_oracle/main.cc",
            "openpilot/system/loggerd/encoder/ffmpeg_encoder.h",
            "openpilot/system/loggerd/encoder/jpeg_encoder.h",
            "openpilot/system/loggerd/encoder/ffmpeg_encoder.cc",
            "openpilot/system/loggerd/encoder/jpeg_encoder.cc",
        ] {
            let destination = source.join(path);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(root.join(path), destination).unwrap();
        }
        let native = base.join("native");
        let library = native.join("usr/lib/x86_64-linux-gnu");
        fs::create_dir_all(&library).unwrap();
        for name in [
            "libavcodec.so.60.31.102",
            "libavutil.so.58.29.100",
            "libyuv.a",
        ] {
            fs::write(library.join(name), name).unwrap();
        }
        let jpeg = base.join("jpeg");
        fs::create_dir(&jpeg).unwrap();
        fs::write(jpeg.join("libjpeg.a"), b"jpeg-library").unwrap();
        let binary = base.join("rust-probe");
        fs::write(&binary, b"native-probe").unwrap();
        let name = case["name"].as_str().unwrap();
        let output = base.join("out");
        if name == "missing-final-library" {
            fs::remove_file(library.join("libavutil.so.58.29.100")).unwrap();
        }
        if name == "existing-output" {
            fs::create_dir(&output).unwrap();
        }
        let options = Options {
            root: source,
            output: output.clone(),
            binary,
            native,
            jpeg,
            library_path: Some("/inherited libs".into()),
        };
        let mut peer = Peer {
            scenario: name.into(),
            base: base.into(),
            commands: vec![],
        };
        let mut stdout = vec![];
        let result = execute(
            &options,
            &mut peer,
            &mut |_| {
                Ok(if name == "low-space" {
                    1
                } else {
                    100 * 1024 * 1024 * 1024
                })
            },
            &mut stdout,
        );
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{name}: {result:?}"
        );
        assert_eq!(
            normalize(&json!(peer.commands), base),
            case["commands"],
            "{name}: commands"
        );
        assert_eq!(
            normalize(&json!(String::from_utf8(stdout).unwrap()), base),
            case["stdout"],
            "{name}: stdout"
        );
        let mut actual = BTreeMap::new();
        files(base, &output, &mut actual);
        assert_eq!(
            json!(actual),
            growth_only_files(&case["files"]),
            "{name}: partial artifacts"
        );
    }
}

#[test]
fn native_codec_cli_requires_options_and_creates_nothing_for_parser_errors() {
    let directory = tempfile::tempdir().unwrap();
    for (arguments, expected) in [
        (vec!["--help"], 0),
        (vec![], 2),
        (vec!["--rust"], 2),
        (vec!["--unknown"], 2),
        (
            vec![
                "--output",
                "out",
                "--rust",
                "binary",
                "--jpeg-build",
                "jpeg",
                "--native",
                "native",
                "extra",
            ],
            2,
        ),
    ] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-encoder-codecs"))
            .args(&arguments)
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(expected), "{arguments:?}");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}

#[test]
fn original_cli_resolves_each_path_at_its_execution_phase() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    // Independent original main capture: source SHA351f01e48543de15cfa22fa2d7eb03d3a7b9f0260fb725c50bf56da5f7bc18c4.
    // Actual shell endpoints replace only pkg-config/compiler/codec; Python
    // Path.resolve and all original file writes remain active in the capture.
    for (argument, phases, expected_files) in [
        ("output", "", vec![]),
        ("native", "", vec!["source-oracle.cc"]),
        ("jpeg-build", "flags\n", vec!["source-oracle.cc"]),
        (
            "rust",
            "flags\nbuild\nsource\n",
            vec![
                "build-command.json",
                "build.log",
                "case-0-lossless-source.log",
                "case-0-lossless-source/packet-00.bin",
                "case-0-lossless-source/trace.tsv",
                "source-oracle",
                "source-oracle.cc",
                "space.json",
            ],
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path();
        let tools = base.join("tools");
        fs::create_dir(&tools).unwrap();
        for (name, script) in [
            ("pkg-config", "#!/bin/sh\necho flags >> \"$PHASE_LOG\"\n"),
            ("g++", "#!/bin/sh\necho build >> \"$PHASE_LOG\"\nfor arg do dest=\"$arg\"; done\n/bin/cat > \"$dest\" <<'PEER'\n#!/bin/sh\necho source >> \"$PHASE_LOG\"\n/bin/mkdir \"$1\"\nprintf 'trace\\n' > \"$1/trace.tsv\"\nprintf packet > \"$1/packet-00.bin\"\nPEER\n/bin/chmod +x \"$dest\"\n"),
        ] {
            let path = tools.join(name);
            fs::write(&path, script).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let looping = base.join("loop");
        symlink("loop", &looping).unwrap();
        let output = base.join("out");
        let log = base.join("phases");
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_check-encoder-codecs"));
        command.env("PATH", &tools).env("PHASE_LOG", &log);
        command.env(
            "OPENPILOT_RUST_ROOT",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        );
        for (name, path) in [
            ("output", output.clone()),
            ("native", base.join("native")),
            ("jpeg-build", base.join("jpeg")),
            ("rust", base.join("candidate")),
        ] {
            command
                .arg(format!("--{name}"))
                .arg(if name == argument { &looping } else { &path });
        }
        let result = command.output().unwrap();
        assert_eq!(result.status.code(), Some(1), "{argument}");
        assert_eq!(
            fs::read_to_string(&log).unwrap_or_default(),
            phases,
            "{argument}: phases"
        );
        let mut actual = BTreeMap::new();
        files(base, &output, &mut actual);
        assert_eq!(
            actual.keys().map(String::as_str).collect::<Vec<_>>(),
            expected_files,
            "{argument}: artifacts"
        );
        assert!(!output.join("receipt.json").exists());
    }
}
