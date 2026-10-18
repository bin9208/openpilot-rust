//! Unchanged host ARM target validator captures; no target hardware involved.
use openpilot_dev_tools::{
    encoder_target_check::{arm_elf, command, execute, loaded_libraries, Options},
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
fn bytes(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn write(path: &Path, data: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap();
}
fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn norm(v: &Value, root: &Path) -> Value {
    serde_json::from_str(
        &serde_json::to_string(v)
            .unwrap()
            .replace(root.to_str().unwrap(), "$ROOT"),
    )
    .unwrap()
}
fn elf() -> Vec<u8> {
    [b"\x7fELF\x02\x01".as_slice(), &[0; 12], b"\xb7\x00"].concat()
}
#[test]
fn independent_target_capture_bytes_are_immutable() {
    assert_eq!(
        hash(include_bytes!(
            "fixtures/encoder-target-check/primitive-original.json"
        )),
        "2bfb0964010fb33f824c5e706c9a9f78fb9ae2c9659dbc785e862a27107c3191"
    );
    assert_eq!(
        hash(include_bytes!(
            "fixtures/encoder-target-check/run-original.json"
        )),
        "e547b433b24ff00f6a5bc2fad2df71959490bc733d320d85578b4c6b0beb4831"
    );
    assert_eq!(
        hash(include_bytes!(
            "fixtures/encoder-target-check/command-original.json"
        )),
        "e6f607bd6fe2ed3b0e46badf41896dbe4454578b6da1fc1f653f94bca420d37b"
    );
}
#[test]
fn original_arm_elf_and_agnos_loader_admission() {
    let corpus: Value = serde_json::from_str(include_str!(
        "fixtures/encoder-target-check/primitive-original.json"
    ))
    .unwrap();
    for c in corpus["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let result = if c["kind"] == "elf" {
            let p = root.join(c["name"].as_str().unwrap());
            write(&p, bytes(c["bytes"].as_str().unwrap()));
            arm_elf(&p).map(|_| Value::Null)
        } else {
            for p in c["files"].as_array().unwrap() {
                let p = p.as_str().unwrap();
                write(&root.join(p), format!("library:{p}"));
            }
            loaded_libraries(
                c["output"]
                    .as_str()
                    .unwrap()
                    .replace("$ROOT", root.to_str().unwrap())
                    .as_bytes(),
                &root.join("agnos"),
            )
            .map(|v| serde_json::to_value(v).unwrap())
        };
        assert_eq!(
            result.is_err(),
            !c["error"].is_null(),
            "{} {result:?}",
            c["name"]
        );
        if let Err(error) = &result {
            if c["error"]["type"] == "TargetFailure" {
                assert_eq!(
                    error.to_string().replace(root.to_str().unwrap(), "$ROOT"),
                    c["error"]["message"].as_str().unwrap()
                );
            }
        }
        if let Ok(v) = result {
            if c["kind"] == "loader" {
                assert_eq!(norm(&v, root), c["result"]);
            }
        }
    }
}
fn setup(root: &Path, scenario: &str) -> Options {
    for (rel, text) in [
        ("rust/tools/encoder_oracle/adapter.h", "adapter\n"),
        ("rust/tools/encoder_oracle/main.cc", "main\n"),
        (
            "openpilot/system/loggerd/encoder/ffmpeg_encoder.h",
            "preamble\nclass FfmpegEncoder\n",
        ),
        (
            "openpilot/system/loggerd/encoder/jpeg_encoder.h",
            "preamble\nclass JpegEncoder\n",
        ),
        (
            "openpilot/system/loggerd/encoder/ffmpeg_encoder.cc",
            "preamble\nFfmpegEncoder::FfmpegEncoder\n",
        ),
        (
            "openpilot/system/loggerd/encoder/jpeg_encoder.cc",
            "preamble\nJpegEncoder::JpegEncoder\n",
        ),
        (
            "rust/crates/dev-tools/src/encoder_target_check.rs",
            "native-target-checker",
        ),
        (
            "rust/crates/dev-tools/src/bin/check_encoder_target.rs",
            "native-target-cli",
        ),
    ] {
        write(&root.join(rel), text);
    }
    let mut opts = Options {
        rust: root.join("rust-elf"),
        rust_sha256: hash(&elf()).into(),
        jpeg_build: root.join("jpeg-build"),
        jpeg_archive: root.join("jpeg.a"),
        jpeg_sha256: hash(b"jpeg").into(),
        ffmpeg: root.join("ffmpeg"),
        ffmpeg_receipt: root.join("ffmpeg-receipt.json"),
        libyuv: root.join("yuv"),
        libyuv_receipt: root.join("yuv-receipt.json"),
        compiler: root.join("compiler"),
        compiler_host_libs: root.join("host-libs"),
        qemu: root.join("qemu"),
        agnos_libs: root.join("agnos"),
        output: root.join("output"),
        pin_receipt: vec![root.join("pin.json")],
    };
    for (p, data) in [
        (&opts.rust, elf()),
        (&opts.jpeg_archive, b"jpeg".to_vec()),
        (&opts.compiler, b"compiler".to_vec()),
        (&opts.qemu, b"qemu".to_vec()),
        (&opts.agnos_libs.join("ld-linux-aarch64.so.1"), elf()),
        (&opts.agnos_libs.join("libc.so"), b"libc".to_vec()),
        (&root.join("pin.json"), b"pin".to_vec()),
    ] {
        write(p, data);
    }
    for d in [
        opts.ffmpeg.join("include"),
        opts.libyuv.join("include"),
        root.join("rust/crates/jpeg/native/vendor/src"),
        opts.jpeg_build.clone(),
    ] {
        write(&d.join("fixture.h"), "header");
    }
    for n in [
        "avformat",
        "avcodec",
        "avutil",
        "swresample",
        "x264",
        "z",
        "va",
        "va-drm",
        "drm",
    ] {
        write(
            &opts.ffmpeg.join(format!("lib/lib{n}.a")),
            format!("archive:{n}"),
        );
    }
    write(&opts.libyuv.join("lib/libyuv.a"), "yuv");
    let mut files = vec![];
    for rel in [
        "include/fixture.h",
        "lib/libavformat.a",
        "lib/libavcodec.a",
        "lib/libavutil.a",
        "lib/libswresample.a",
        "lib/libx264.a",
        "lib/libz.a",
        "lib/libva.a",
        "lib/libva-drm.a",
        "lib/libdrm.a",
    ] {
        files.push(json!({"path":format!("ffmpeg-7.1.0.data/purelib/ffmpeg/install/{rel}"),"sha256":hash(&fs::read(opts.ffmpeg.join(rel)).unwrap())}));
    }
    files.sort_by_key(|v| v["path"].as_str().unwrap().to_owned());
    let mut ff = json!({"wheel_sha256":"ce758b64c0343574e18ab97cbcff1e29868c66ccc5e0c866b5970266451203d4","files":files});
    let mut yuv = json!({"sha256":"1659e55a357f732836e5ed2a17fa65e715a11bf9bfa6d305e71fd7ac00a8e475","files":{"extracted/libyuv-1922.0.data/purelib/libyuv/install/include/fixture.h":hash(b"header"),"extracted/libyuv-1922.0.data/purelib/libyuv/install/lib/libyuv.a":hash(b"yuv")}});
    match scenario {
        "ffmpeg-pin" => ff["wheel_sha256"] = "bad".into(),
        "yuv-pin" => yuv["sha256"] = "bad".into(),
        "ffmpeg-payload" => ff["files"][0]["sha256"] = "bad".into(),
        "yuv-payload" => {
            yuv["files"]["extracted/libyuv-1922.0.data/purelib/libyuv/install/include/fixture.h"] =
                "bad".into()
        }
        "receipt-missing-field" => {
            ff.as_object_mut().unwrap().remove("wheel_sha256");
        }
        _ => {}
    }
    write(&opts.ffmpeg_receipt, ff.to_string());
    write(&opts.libyuv_receipt, yuv.to_string());
    match scenario {
        "rust-hash" => opts.rust_sha256 = "bad".into(),
        "jpeg-hash" => opts.jpeg_sha256 = "bad".into(),
        "rust-elf" => {
            write(&opts.rust, "not-elf");
            opts.rust_sha256 = hash(b"not-elf").into();
        }
        "loader-elf" => write(&opts.agnos_libs.join("ld-linux-aarch64.so.1"), "not-elf"),
        "missing-include" => fs::remove_dir_all(&opts.jpeg_build).unwrap(),
        _ => {}
    }
    opts
}
struct Peer {
    root: PathBuf,
    scenario: String,
    calls: std::rc::Rc<std::cell::RefCell<Vec<Value>>>,
}
impl Runner for Peer {
    fn run(&mut self, s: &CommandSpec) -> Result<Captured> {
        assert_eq!(s.cwd.as_deref(), Some(self.root.as_path()));
        assert!(s.disable_core_dump);
        assert_eq!(s.timeout, std::time::Duration::from_secs(300));
        assert!(s.inherit_stdin);
        let argv = s
            .effective_argv()
            .iter()
            .map(|v| v.to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let build = argv[0] == self.root.join("compiler").to_str().unwrap();
        let directory = if build {
            self.root.join("output/build")
        } else if argv.iter().any(|v| v == "--list") {
            self.root
                .join(if argv.last().unwrap().ends_with("source-oracle") {
                    "output/loader-source"
                } else {
                    "output/loader-rust"
                })
        } else {
            let target = Path::new(&argv[5]);
            target
                .parent()
                .unwrap()
                .join(if target.file_name().unwrap() == "source" {
                    "run-source"
                } else {
                    "run-rust"
                })
        };
        let env = s.effective_env();
        for key in ["LD_PRELOAD", "QEMU_FIXTURE"] {
            assert!(!env.contains_key(std::ffi::OsStr::new(key)));
        }
        let mut e = serde_json::Map::new();
        for k in [
            "LC_ALL",
            "DEBUG_ENCODER",
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "QEMU_FIXTURE",
        ] {
            e.insert(
                k.into(),
                env.get(std::ffi::OsStr::new(k))
                    .map(|v| json!(v.to_str().unwrap()))
                    .unwrap_or(Value::Null),
            );
        }
        self.calls
            .borrow_mut()
            .push(json!(["command", directory, argv, e]));
        let name = directory.file_name().unwrap().to_str().unwrap();
        if (self.scenario == "build-fail" && name == "build")
            || (self.scenario == "source-fail" && name == "run-source")
        {
            return Err("controlled child failure".into());
        }
        let mut stdout = vec![];
        let mut stderr = b"encoder-target b08d7969 4002660 3999588 3876708 1922\n".to_vec();
        if build {
            write(
                &self.root.join("output/source-oracle"),
                if self.scenario == "oracle-elf" {
                    b"bad".to_vec()
                } else {
                    elf()
                },
            );
        } else if name.starts_with("loader") {
            if self.scenario == "loader-empty" {
                stdout = b"no resolved libraries\n".to_vec();
            } else {
                let p = if self.scenario == "loader-outside" {
                    self.root.join("outside.so")
                } else {
                    self.root.join("agnos/libc.so")
                };
                write(&p, "libc");
                stdout = format!("{} (0x1)\n", p.display()).into_bytes();
            }
        } else {
            let target = Path::new(&argv[5]);
            fs::create_dir(target)?;
            write(
                &target.join("trace.tsv"),
                if self.scenario == "trace-different" && name == "run-rust" {
                    b"different".as_slice()
                } else {
                    b"trace\n"
                },
            );
            let number = target
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .split('-')
                .nth(1)
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let mut count = [16, 16, 68, 6, 6][number];
            if name == "run-rust" {
                if self.scenario == "packet-extra" {
                    count += 1;
                }
                if self.scenario == "packet-missing" {
                    count -= 1;
                }
            }
            for i in 0..count {
                write(
                    &target.join(format!("packet-{i:03}.bin")),
                    if self.scenario == "packet-bytes" && name == "run-rust" && i == 0 {
                        "changed".into()
                    } else {
                        format!("packet:{i}")
                    },
                );
            }
            if self.scenario == "runtime-version" {
                stderr = b"wrong versions\n".to_vec();
            }
            if self.scenario == "rust-changed" && number == 4 && name == "run-rust" {
                write(
                    &self.root.join("rust-elf"),
                    [elf(), b"changed".to_vec()].concat(),
                );
            }
        }
        Ok(Captured {
            code: 0,
            stdout,
            stderr,
        })
    }
}
#[test]
fn original_full_target_matrix_and_failure_receipts() {
    let corpus: Value = serde_json::from_str(include_str!(
        "fixtures/encoder-target-check/run-original.json"
    ))
    .unwrap();
    for c in corpus["cases"].as_array().unwrap() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        let name = c["name"].as_str().unwrap();
        let options = setup(root, name);
        let calls = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
        let mut peer = Peer {
            root: root.into(),
            scenario: name.into(),
            calls: calls.clone(),
        };
        let observed = calls.clone();
        let mut out = vec![];
        let result = execute(
            root,
            &options,
            &mut peer,
            &mut |path: &Path, growth: u64| -> Result<()> {
                observed.borrow_mut().push(json!(["disk", path, growth]));
                if name == "disk" {
                    Err("insufficient disk space; recover at least 35 GiB before resuming".into())
                } else {
                    Ok(())
                }
            },
            &[
                ("LD_PRELOAD".into(), "forbidden-preload".into()),
                ("LD_LIBRARY_PATH".into(), "forbidden-library".into()),
                ("QEMU_FIXTURE".into(), "forbidden-qemu".into()),
                ("LC_ALL".into(), "fixture".into()),
                ("DEBUG_ENCODER".into(), "fixture".into()),
            ]
            .into(),
            &mut out,
        );
        assert_eq!(result.is_err(), !c["error"].is_null(), "{name}: {result:?}");
        // Command implementation now owns its per-child disk guard. Captured original
        // fullrun replaced command entirely; remove only those newly visible256MiB
        // child checks from this call-order adapter (separate command tests assert them).
        let history = calls
            .borrow()
            .iter()
            .filter(|v| !(v[0] == "disk" && v[2] == 256 * 1024 * 1024))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(norm(&json!(history), root), c["calls"], "{name}");
        let receipt: Value =
            serde_json::from_slice(&fs::read(options.output.join("receipt.json")).unwrap())
                .unwrap();
        assert_eq!(receipt["status"], c["status"]);
        assert_eq!(receipt["completed_cases"], c["completed"]);
        assert_eq!(receipt["required_cases"], c["required_cases"]);
        assert_eq!(
            receipt["required_packet_counts"],
            c["required_packet_counts"]
        );
        if let Some(expected) = c["generated"].as_str() {
            assert_eq!(
                fs::read_to_string(options.output.join("source-oracle.cc")).unwrap(),
                expected
            );
        }
        if c["error"].is_null() {
            assert_eq!(
                norm(&json!(String::from_utf8(out).unwrap()), root),
                json!(c["stdout"])
            );
            assert!(receipt["artifacts"].as_array().unwrap().len() > 224);
        }
        assert_eq!(
            options.output.join("failure.json").exists(),
            !c["failure"].is_null()
        );
        if !c["failure"].is_null() {
            let failure: Value =
                serde_json::from_slice(&fs::read(options.output.join("failure.json")).unwrap())
                    .unwrap();
            assert_eq!(norm(&failure, root), c["failure"], "{name}");
        }
    }
}
struct CommandPeer {
    timeout: bool,
}
impl Runner for CommandPeer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        if self.timeout {
            return Err(openpilot_dev_tools::qa_process::CaptureError::Timeout {
                timeout: std::time::Duration::from_secs(300),
                stdout: vec![],
                stderr: vec![],
            }
            .into());
        }
        NativeRunner.run(spec)
    }
}
#[test]
fn original_actual_command_binary_logs_exit_and_execerror() {
    let corpus: Value = serde_json::from_str(include_str!(
        "fixtures/encoder-target-check/command-original.json"
    ))
    .unwrap();
    for c in corpus["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        let directory = root.join("command");
        let argv = c["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect::<Vec<_>>();
        let env = c["environment"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone().into(), v.as_str().unwrap().into()))
            .collect::<BTreeMap<_, _>>();
        let mut disk = vec![];
        let result = command(
            root,
            &directory,
            argv,
            env,
            &mut CommandPeer {
                timeout: name == "timeout",
            },
            &mut |p: &Path, growth: u64| -> Result<()> {
                disk.push(json!([p, growth]));
                if name == "disk" {
                    Err("fixture disk refusal".into())
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(result.is_err(), !c["error"].is_null(), "{name}");
        assert_eq!(norm(&json!(disk), root), c["disk"]);
        for (file, expected) in c["artifacts"].as_object().unwrap() {
            let actual = fs::read(directory.join(file)).unwrap();
            if file == "command.json" {
                let mut v: Value = serde_json::from_slice(&actual).unwrap();
                let o = v.as_object_mut().unwrap();
                assert!(o.remove("started_monotonic_ns").unwrap().as_u64().unwrap() > 0);
                assert!(o.remove("ended_monotonic_ns").unwrap().as_u64().unwrap() > 0);
                assert_eq!(norm(&v, root), *expected);
            } else {
                assert_eq!(actual, bytes(expected.as_str().unwrap()));
            }
        }
    }
}

#[test]
fn real_child_environment_is_exactly_the_supplied_sanitized_map() {
    const PEER: &str = "ENCODER_TARGET_ENVIRONMENT_PEER";
    if std::env::var_os(PEER).is_some() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let inherited = std::env::vars_os().collect::<BTreeMap<_, _>>();
        assert!(inherited.keys().any(|k| k == "QEMU_UNRELATED_TARGET_FLAG"));
        let sanitized = inherited
            .into_iter()
            .filter(|(k, _)| {
                !k.as_encoded_bytes().starts_with(b"QEMU_")
                    && k != "LD_PRELOAD"
                    && k != "LD_LIBRARY_PATH"
            })
            .collect::<BTreeMap<_, _>>();
        let actual=command(root,&root.join("child"),vec!["/bin/sh".into(),"-c".into(),"set -e; test -z \"${QEMU_UNRELATED_TARGET_FLAG+x}${LD_PRELOAD+x}${LD_LIBRARY_PATH+x}\"; printf clean".into()],sanitized,&mut NativeRunner,&mut|_,_|Ok(())).unwrap();
        assert_eq!(actual, b"clean");
        return;
    }
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "real_child_environment_is_exactly_the_supplied_sanitized_map",
            "--nocapture",
        ])
        .env(PEER, "1")
        .env("QEMU_UNRELATED_TARGET_FLAG", "must-not-reach-child")
        .env("LD_LIBRARY_PATH", "/owned/inherited/contamination")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn log_open_failure_names_the_log_path_and_preserves_open_order() {
    let original = include_bytes!("fixtures/encoder-target-check/log-open-original.json");
    assert_eq!(
        hash(original),
        "df6a9ef3d585ced45933400e6ad15190b850deaefdd6bf2be27dfb8f58288398"
    );
    let corpus: Value = serde_json::from_slice(original).unwrap();
    for obstructed in ["stdout.bin", "stderr.bin"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let output = root.join("command");
        let marker = root.join("should-not-spawn");
        let result = command(
            root,
            &output,
            vec![
                "/bin/sh".into(),
                "-c".into(),
                format!("touch '{}'", marker.display()).into(),
            ],
            [
                ("PATH".into(), "/usr/bin:/bin".into()),
                ("LC_ALL".into(), "C".into()),
                ("DEBUG_ENCODER".into(), "0".into()),
            ]
            .into(),
            &mut NativeRunner,
            &mut |directory, _| {
                fs::create_dir(directory.join(obstructed))?;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!marker.exists());
        let receipt: Value =
            serde_json::from_slice(&fs::read(output.join("command.json")).unwrap()).unwrap();
        assert_eq!(receipt["status"], "EXEC_ERROR");
        assert!(receipt["returncode"].is_null());
        assert_eq!(
            receipt["error"],
            format!(
                "[Errno 21] Is a directory: '{}'",
                output.join(obstructed).display()
            )
        );
        let expected = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == format!("{obstructed}-directory"))
            .unwrap();
        let mut observed = receipt.clone();
        let fields = observed.as_object_mut().unwrap();
        assert!(
            fields
                .remove("started_monotonic_ns")
                .unwrap()
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(
            fields
                .remove("ended_monotonic_ns")
                .unwrap()
                .as_u64()
                .unwrap()
                > 0
        );
        fields.insert("cwd".into(), "$ROOT".into());
        let observed: Value = serde_json::from_str(
            &serde_json::to_string(&observed)
                .unwrap()
                .replace(root.to_str().unwrap(), &format!("$TEMP/{obstructed}")),
        )
        .unwrap();
        assert_eq!(observed, expected["artifacts"]["command.json"]);
        if obstructed == "stderr.bin" {
            assert!(fs::read(output.join("stdout.bin")).unwrap().is_empty());
        } else {
            assert!(!output.join("stderr.bin").exists());
        }
    }
}

#[test]
fn original_raw_unix_hash_cli_finalizes_lossless_failure_receipts() {
    use openpilot_log_format::{JsonValue, JsonView};
    use std::os::unix::ffi::OsStringExt;
    let raw = include_bytes!("fixtures/encoder-target-check/raw-hash-cli-original.json");
    assert_eq!(
        hash(raw),
        "88d77a81409d0a41b269aee5ee3d77a53333ca541faaf87ff73457f7585fbfbf"
    );
    let corpus: Value = serde_json::from_slice(raw).unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let options = setup(root, "success");
        let key = case["name"].as_str().unwrap();
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_check-encoder-target"));
        child.env("OPENPILOT_RUST_ROOT", root);
        for (name, path) in [
            ("--rust", &options.rust),
            ("--jpeg-build", &options.jpeg_build),
            ("--jpeg-archive", &options.jpeg_archive),
            ("--ffmpeg", &options.ffmpeg),
            ("--ffmpeg-receipt", &options.ffmpeg_receipt),
            ("--libyuv", &options.libyuv),
            ("--libyuv-receipt", &options.libyuv_receipt),
            ("--compiler", &options.compiler),
            ("--compiler-host-libs", &options.compiler_host_libs),
            ("--qemu", &options.qemu),
            ("--agnos-libs", &options.agnos_libs),
            ("--output", &options.output),
            ("--pin-receipt", &options.pin_receipt[0]),
        ] {
            child.arg(name).arg(path);
        }
        for (name, value) in [
            ("rust_sha256", &options.rust_sha256),
            ("jpeg_sha256", &options.jpeg_sha256),
        ] {
            child.arg(format!("--{}", name.replace('_', "-")));
            if name == key {
                child.arg(std::ffi::OsString::from_vec(vec![255]));
            } else {
                child.arg(value);
            }
        }
        let result = child.output().unwrap();
        assert_eq!(
            result.status.code(),
            case["exit"].as_i64().map(|v| v as i32)
        );
        assert_eq!(result.stdout, bytes(case["stdout_hex"].as_str().unwrap()));
        let failure: Value =
            serde_json::from_slice(&fs::read(options.output.join("failure.json")).unwrap())
                .unwrap();
        assert_eq!(failure, case["failure"]);
        let receipt =
            JsonValue::parse(&fs::read_to_string(options.output.join("receipt.json")).unwrap())
                .unwrap();
        assert_eq!(
            receipt.get("status").unwrap().to_utf8().as_deref(),
            Some("FAIL")
        );
        let value = receipt.get("options").unwrap().get(key).unwrap();
        let JsonView::Text(points) = value.view() else {
            panic!("raw hash must be text")
        };
        let expected = case["option_codepoints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect::<Vec<_>>();
        assert_eq!(points, expected);
    }
}

#[test]
fn executing_checker_identity_changes_for_owned_mutated_binary_copy() {
    use std::{io::Write, os::unix::fs::PermissionsExt};
    const ISOLATED: &str = "OPENPILOT_ENCODER_IDENTITY_CHILD";
    if std::env::var_os(ISOLATED).as_deref() != Some(std::ffi::OsStr::new("1")) {
        // A concurrent test fork can inherit a just-written executable's writer
        // before close-on-exec runs. Keep the real copy/hash check, but create
        // its writers in an isolated process with no unrelated test spawns.
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "executing_checker_identity_changes_for_owned_mutated_binary_copy",
                "--test-threads=1",
                "--nocapture",
            ])
            .env(ISOLATED, "1")
            .output()
            .unwrap();
        assert!(
            result.status.success()
                && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"),
            "isolated identity check failed: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut identities = vec![];
    for index in 0..2 {
        let case_root = root.join(format!("case-{index}"));
        let options = setup(&case_root, "rust-hash");
        let executable = root.join(format!("owned-checker-{index}"));
        fs::copy(env!("CARGO_BIN_EXE_check-encoder-target"), &executable).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        if index == 1 {
            fs::OpenOptions::new()
                .append(true)
                .open(&executable)
                .unwrap()
                .write_all(b"owned identity mutation\n")
                .unwrap();
        }
        let mut child = std::process::Command::new(&executable);
        child.env("OPENPILOT_RUST_ROOT", &case_root);
        for (name, path) in [
            ("--rust", &options.rust),
            ("--jpeg-build", &options.jpeg_build),
            ("--jpeg-archive", &options.jpeg_archive),
            ("--ffmpeg", &options.ffmpeg),
            ("--ffmpeg-receipt", &options.ffmpeg_receipt),
            ("--libyuv", &options.libyuv),
            ("--libyuv-receipt", &options.libyuv_receipt),
            ("--compiler", &options.compiler),
            ("--compiler-host-libs", &options.compiler_host_libs),
            ("--qemu", &options.qemu),
            ("--agnos-libs", &options.agnos_libs),
            ("--output", &options.output),
            ("--pin-receipt", &options.pin_receipt[0]),
        ] {
            child.arg(name).arg(path);
        }
        let result = child
            .args(["--rust-sha256", "bad", "--jpeg-sha256", &hash(b"jpeg")])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        let receipt: Value =
            serde_json::from_slice(&fs::read(options.output.join("receipt.json")).unwrap())
                .unwrap();
        let identity = receipt["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["path"] == executable.to_str().unwrap())
            .expect("executing native checker identity");
        let expected = hash(&fs::read(&executable).unwrap());
        assert_eq!(identity["sha256"], expected);
        identities.push(expected);
        let failure: Value =
            serde_json::from_slice(&fs::read(options.output.join("failure.json")).unwrap())
                .unwrap();
        assert_eq!(
            failure,
            json!({"type":"TargetFailure","error":"frozen Rust ELF hash differs"})
        );
        let rust_identity = receipt["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["path"] == options.rust.to_str().unwrap())
            .unwrap();
        assert_eq!(rust_identity["sha256"], hash(&elf()));
    }
    assert_ne!(identities[0], identities[1]);
}
