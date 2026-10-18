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
        assert_eq!(json!(actual), case["files"], "{name}: partial artifacts");
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
