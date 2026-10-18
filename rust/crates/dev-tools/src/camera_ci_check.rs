//! Camera source build and comparison orchestration; every oracle remains live.
use crate::{
    python_fs,
    qa_process::{CommandSpec, Runner},
    seal::Result,
};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
const ASAN: &str = "/usr/lib/llvm-18/lib/clang/18/lib/linux";
const POLICIES: [(&str, &str, &str); 5] = [
    ("sensor", "sensors", "sensor_policy"),
    ("exposure", "exposure", "exposure_policy"),
    ("misc", "misc", "misc_policy"),
    ("requests", "requests", "request_policy"),
    ("packets", "packets", "sensor_packets"),
];
const LIFECYCLES: [&str; 4] = [
    "kernel",
    "sensor_lifecycle",
    "isp_lifecycle",
    "camera_lifecycle",
];
pub const CASES: [&str; 8] = [
    "term",
    "int",
    "pwr",
    "disabled-road",
    "missing-road",
    "wait-failure",
    "poll-error",
    "publication-order",
];
const SCHEMAS: [&str; 4] = ["car", "custom", "deprecated", "log"];
pub struct Options {
    pub root: PathBuf,
    pub output: PathBuf,
    pub binaries: PathBuf,
    pub compiler: OsString,
}
struct Check<'a> {
    options: &'a Options,
    runner: &'a mut dyn Runner,
    space: &'a mut dyn FnMut(&Path, u64) -> Result<()>,
    printed: &'a mut dyn Write,
    errors: &'a mut dyn Write,
}
fn os(value: impl AsRef<OsStr>) -> OsString {
    value.as_ref().to_owned()
}
fn prefixed(prefix: &str, path: &Path) -> OsString {
    let mut out = OsString::from(prefix);
    out.push(path);
    out
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn argv_json(argv: &[OsString]) -> Result<String> {
    Ok(format!(
        "[\n{}\n  ]",
        argv.iter()
            .map(|v| Ok(format!("    {}", python_fs::json_string(v)?)))
            .collect::<Result<Vec<_>>>()?
            .join(",\n")
    ))
}
impl Check<'_> {
    fn command(&mut self, name: &str, argv: Vec<OsString>, growth: u64) -> Result<PathBuf> {
        (self.space)(&self.options.output, growth * 1024 * 1024)?;
        let directory = self.options.output.join("commands");
        fs::create_dir_all(&directory)?;
        fs::write(
            directory.join(format!("{name}.command.json")),
            format!(
                "{{\n  \"argv\": {},\n  \"cwd\": {}\n}}\n",
                argv_json(&argv)?,
                python_fs::json_string(self.options.root.as_os_str())?
            ),
        )?;
        let mut spec = CommandSpec::new_os(argv, Duration::MAX);
        spec.cwd = Some(self.options.root.clone());
        spec.inherit_stdin = true;
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        let captured = self.runner.run(&spec)?;
        let stdout = text(captured.stdout)?;
        let stderr = text(captured.stderr)?;
        let target = directory.join(format!("{name}.stdout"));
        fs::write(&target, &stdout)?;
        fs::write(directory.join(format!("{name}.stderr")), &stderr)?;
        fs::write(
            directory.join(format!("{name}.exit.json")),
            format!("{{\"returncode\": {}}}\n", captured.code),
        )?;
        if captured.code != 0 {
            writeln!(self.errors, "{stdout}{stderr}")?;
            return Err(format!("camera command {name} exited {}", captured.code).into());
        }
        writeln!(self.printed, "PASS {name}")?;
        self.printed.flush()?;
        Ok(target)
    }
    fn generate(&mut self, name: &str, program: &str, arguments: &[&str]) -> Result<PathBuf> {
        let mut argv = vec![
            os(self.options.binaries.join(program)),
            "--source".into(),
            os(&self.options.root),
        ];
        argv.extend(arguments.iter().map(os));
        let source = self.command(&format!("generate-{name}"), argv, 64)?;
        let target = self.options.output.join(format!("{name}.cc"));
        fs::copy(source, &target)?;
        Ok(target)
    }
    fn compile(
        &mut self,
        name: &str,
        sources: &[PathBuf],
        sanitized: bool,
        shared: bool,
        libraries: &[&str],
    ) -> Result<PathBuf> {
        let mut argv = vec![
            self.options.compiler.clone(),
            "-std=c++17".into(),
            if sanitized { "-O1" } else { "-O2" }.into(),
            "-pthread".into(),
        ];
        for directory in [
            self.options.root.join("openpilot"),
            self.options.root.join("third_party/linux/include"),
            self.options.output.join("include"),
            self.options.output.clone(),
            self.options.output.join("cereal/gen/cpp"),
            self.options.root.join("msgq_repo"),
        ] {
            argv.push(prefixed("-I", &directory));
        }
        if sanitized {
            argv.extend(
                [
                    "-g",
                    "-fno-omit-frame-pointer",
                    "-fsanitize=address,undefined",
                    "-shared-libasan",
                ]
                .map(OsString::from),
            );
            argv.push(format!("-Wl,-rpath,{ASAN}").into());
        }
        if shared {
            argv.extend(["-shared".into(), "-fPIC".into()]);
        }
        let target = self
            .options
            .output
            .join(format!("{name}{}", if shared { ".so" } else { "" }));
        argv.extend(sources.iter().map(os));
        argv.extend(libraries.iter().map(os));
        argv.extend(["-o".into(), os(&target)]);
        self.command(&format!("compile-{name}"), argv, 256)?;
        Ok(target)
    }
    fn stage(&mut self) -> Result<()> {
        let output = &self.options.output;
        let root = &self.options.root;
        let include = output.join("include/media");
        create_new_tree(&include)?;
        fs::copy(
            root.join("third_party/linux/include/msm_camsensor_sdk.h"),
            include.join("msm_camsensor_sdk.h"),
        )?;
        let schema = output.join("schema");
        create_new_tree(&schema.join("include"))?;
        for name in SCHEMAS {
            let directory = if name == "car" {
                "opendbc_repo/opendbc/car"
            } else {
                "openpilot/cereal"
            };
            fs::copy(
                root.join(directory).join(format!("{name}.capnp")),
                schema.join(format!("{name}.capnp")),
            )?;
        }
        fs::copy(
            root.join("openpilot/cereal/include/c++.capnp"),
            schema.join("include/c++.capnp"),
        )?;
        let generated = output.join("cereal/gen/cpp");
        create_new_tree(&generated)?;
        let mut argv = vec![
            "capnp".into(),
            "compile".into(),
            "-I".into(),
            os(&schema),
            prefixed("--src-prefix=", &schema),
            prefixed("-oc++:", &generated),
        ];
        argv.extend(
            SCHEMAS
                .iter()
                .map(|name| os(schema.join(format!("{name}.capnp")))),
        );
        self.command("compile-schema", argv, 128)?;
        Ok(())
    }
    fn build(&mut self) -> Result<()> {
        self.stage()?;
        let root = &self.options.root;
        let tools = root.join("rust/tools");
        let sensors = ["ar0231", "ox03c10", "os04c10"]
            .map(|name| root.join(format!("openpilot/system/camerad/sensors/{name}.cc")));
        let cdm = root.join("openpilot/system/camerad/cameras/cdm.cc");
        let mut sources = vec![tools.join("camerad_sensor_source.cc")];
        sources.extend(sensors.iter().cloned());
        self.compile("sensor-source", &sources, false, false, &[])?;
        let exposure = self.generate("exposure", "generate-camerad-ae-reference", &[])?;
        let mut sources = vec![exposure];
        sources.extend(sensors.iter().cloned());
        self.compile("exposure-source", &sources, false, false, &[])?;
        let misc = self.generate("misc", "generate-camerad-ae-reference", &["--kind", "misc"])?;
        self.compile("misc-source", &[misc, cdm.clone()], false, false, &[])?;
        let requests = self.generate("requests", "generate-camerad-requests-reference", &[])?;
        self.compile("requests-source", &[requests], false, false, &[])?;
        let packets = self.generate("packets", "generate-camerad-packet-reference", &[])?;
        let mut sources = vec![packets, cdm.clone()];
        sources.extend(sensors.iter().cloned());
        self.compile("packets-source", &sources, false, false, &[])?;
        for name in LIFECYCLES {
            let program = format!("generate-camerad-{}-reference", name.replace('_', "-"));
            let source = self.generate(name, &program, &[])?;
            let mut sources = vec![source];
            if name != "kernel" {
                sources.extend(sensors.iter().cloned());
            }
            if ["isp_lifecycle", "camera_lifecycle"].contains(&name) {
                sources.push(cdm.clone());
            }
            if name == "camera_lifecycle" {
                sources.extend(
                    [
                        "msgq.cc",
                        "ipc.cc",
                        "event.cc",
                        "impl_msgq.cc",
                        "impl_fake.cc",
                        "visionipc/visionipc.cc",
                        "visionipc/visionipc_client.cc",
                        "visionipc/visionipc_server.cc",
                        "visionipc/visionbuf.cc",
                    ]
                    .map(|part| self.options.root.join("msgq_repo/msgq").join(part)),
                );
            }
            self.compile(&format!("{name}-source"), &sources, true, false, &[])?;
            let fixture = if name == "kernel" {
                tools.join("camerad_kernel_fixture.cc")
            } else {
                self.generate(&format!("{name}-fixture"), &program, &["--fixture"])?
            };
            self.compile(&format!("{name}-fixture"), &[fixture], true, true, &[])?;
        }
        let state = self.generate("state", "generate-camerad-state-reference", &[])?;
        let mut sources = vec![state];
        sources.extend(sensors.iter().cloned());
        sources.extend(SCHEMAS.map(|name| {
            self.options
                .output
                .join(format!("cereal/gen/cpp/{name}.capnp.c++"))
        }));
        self.compile("state-source", &sources, true, false, &["-lcapnp", "-lkj"])?;
        let stress = self.generate("stress", "generate-camerad-stress-reference", &[])?;
        self.compile("stress-source", &[stress], false, false, &[])?;
        self.compile(
            "stress-fixture",
            &[tools.join("camerad_stress_fixture.cc")],
            false,
            true,
            &["-ldl"],
        )?;
        let runtime = self.generate("runtime-fixture", "generate-camerad-runtime-fixture", &[])?;
        self.compile("runtime-fixture", &[runtime], true, true, &[])?;
        Ok(())
    }
    fn constants(&mut self) -> Result<()> {
        for (kind, tracked) in [("sensor", "sensor/data.rs"), ("bps", "isp/bps_data.rs")] {
            let mut argv = vec![
                os(self
                    .options
                    .binaries
                    .join(format!("generate-camerad-{kind}-data"))),
                "--source".into(),
                os(&self.options.root),
            ];
            if kind == "sensor" {
                argv.extend([
                    "--reference".into(),
                    os(self.options.output.join("sensor-source")),
                ]);
            }
            let stdout = self.command(&format!("constants-{kind}"), argv, 64)?;
            let generated = self.options.output.join(format!("generated-{kind}.rs"));
            fs::copy(stdout, &generated)?;
            let command = |path: &Path| {
                let mut argv = [
                    "rustfmt",
                    "--quiet",
                    "--edition",
                    "2021",
                    "--emit",
                    "stdout",
                ]
                .map(OsString::from)
                .to_vec();
                argv.push(os(path));
                argv
            };
            let actual =
                self.command(&format!("format-generated-{kind}"), command(&generated), 64)?;
            let expected = self.command(
                &format!("format-tracked-{kind}"),
                command(
                    &self
                        .options
                        .root
                        .join("rust/crates/camerad/src")
                        .join(tracked),
                ),
                64,
            )?;
            if fs::read(actual)? != fs::read(expected)? {
                return Err(format!(
                    "{kind} generated constants differ; inspect retained formatted outputs"
                )
                .into());
            }
        }
        Ok(())
    }
    fn check(&mut self) -> Result<()> {
        let examples = self.options.binaries.join("examples");
        for (reference, checker, binary) in POLICIES {
            self.command(
                &format!("check-{reference}"),
                vec![
                    os(self
                        .options
                        .binaries
                        .join(format!("check-camerad-{checker}"))),
                    "--reference".into(),
                    os(self.options.output.join(format!("{reference}-source"))),
                    "--binary".into(),
                    os(examples.join(binary)),
                    "--output".into(),
                    os(self.options.output.join(format!("result-{reference}"))),
                ],
                512,
            )?;
        }
        for name in LIFECYCLES {
            let binary = if name == "kernel" {
                "kernel_operations"
            } else {
                name
            };
            let command = vec![
                os(self
                    .options
                    .binaries
                    .join(format!("check-camerad-{}", name.replace('_', "-")))),
                "--source".into(),
                os(self.options.output.join(format!("{name}-source"))),
                "--native".into(),
                os(examples.join(binary)),
                "--fixture".into(),
                os(self.options.output.join(format!("{name}-fixture.so"))),
            ];
            let mut argv = command.clone();
            argv.extend([
                "--output".into(),
                os(self.options.output.join(format!("result-{name}"))),
            ]);
            self.command(&format!("check-{name}"), argv, 512)?;
            if name == "camera_lifecycle" {
                let mut argv = command;
                argv.extend([
                    "--cleanup-only".into(),
                    "--output".into(),
                    os(self.options.output.join("result-cleanup")),
                ]);
                self.command("check-camera-cleanup", argv, 512)?;
            }
        }
        self.command(
            "check-state",
            vec![
                os(self.options.binaries.join("check-camerad-state")),
                "--source".into(),
                os(self.options.output.join("state-source")),
                "--native".into(),
                os(examples.join("frame_state")),
                "--schema".into(),
                os(self.options.output.join("schema/log.capnp")),
                "--output".into(),
                os(self.options.output.join("result-state")),
            ],
            512,
        )?;
        self.command(
            "check-stress",
            vec![
                os(self.options.binaries.join("check-camerad-runtime-stress")),
                "--reference".into(),
                os(self.options.output.join("stress-source")),
                "--native".into(),
                os(examples.join("stress_trace")),
                "--parser".into(),
                os(examples.join("stress_parse_trace")),
                "--fixture".into(),
                os(self.options.output.join("stress-fixture.so")),
                "--output".into(),
                os(self.options.output.join("result-stress")),
            ],
            512,
        )?;
        for case in CASES {
            self.command(
                &format!("check-runtime-{case}"),
                vec![
                    os(self.options.binaries.join("check-camerad-runtime")),
                    "--binary".into(),
                    os(self.options.binaries.join("openpilot-camerad")),
                    "--fixture".into(),
                    os(self.options.output.join("runtime-fixture.so")),
                    "--oracle".into(),
                    os(self.options.output.join("state-source")),
                    "--case".into(),
                    case.into(),
                    "--output".into(),
                    os(self.options.output.join(format!("runtime-{case}"))),
                ],
                512,
            )?;
        }
        Ok(())
    }
}
fn create_new_tree(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(path)?;
    Ok(())
}
pub fn execute(
    options: &Options,
    runner: &mut dyn Runner,
    space: &mut dyn FnMut(&Path, u64) -> Result<()>,
    printed: &mut dyn Write,
    errors: &mut dyn Write,
) -> Result<()> {
    let mut check = Check {
        options,
        runner,
        space,
        printed,
        errors,
    };
    check.build()?;
    check.constants()?;
    check.check()
}

fn paths_in(directory: &Path, recursive: bool, paths: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        paths.push(path.clone());
        if recursive && entry.file_type()?.is_dir() {
            paths_in(&path, true, paths)?;
        }
    }
    Ok(())
}
fn source_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = vec![];
    for (directory, prefix, suffix) in [
        ("rust/tools", "", ""),
        ("rust/crates/dev-tools/src", "camera", ".rs"),
        ("rust/crates/dev-tools/src/bin", "generate_camerad_", ".rs"),
    ] {
        let mut entries = vec![];
        paths_in(&root.join(directory), false, &mut entries)?;
        paths.extend(entries.into_iter().filter(|p| {
            let name = p.file_name().unwrap().as_encoded_bytes();
            if directory == "rust/tools" {
                name.windows(7).any(|w| w == b"camerad")
            } else {
                name.starts_with(prefix.as_bytes()) && name.ends_with(suffix.as_bytes())
            }
        }));
    }
    paths.extend(
        [
            "rust/Cargo.lock",
            "rust/crates/dev-tools/Cargo.toml",
            "rust/crates/dev-tools/src/bin/check_camerad_ci.rs",
        ]
        .map(|p| root.join(p)),
    );
    for name in POLICIES.iter().map(|(_, name, _)| *name).chain([
        "kernel",
        "sensor_lifecycle",
        "isp_lifecycle",
        "camera_lifecycle",
        "state",
        "runtime_stress",
        "runtime",
    ]) {
        paths.push(root.join(format!(
            "rust/crates/dev-tools/src/bin/check_camerad_{name}.rs"
        )));
    }
    for directory in [
        "rust/crates/camerad",
        "rust/crates/camera-kernel",
        "rust/crates/camerad-runtime",
        "rust/crates/msgq",
        "openpilot/system/camerad",
        "msgq_repo/msgq",
    ] {
        let mut entries = vec![];
        paths_in(&root.join(directory), true, &mut entries)?;
        paths.extend(entries.into_iter().filter(|p| {
            p.extension().is_some_and(|s| {
                ["rs", "cc", "h", "toml"]
                    .iter()
                    .any(|allowed| s == *allowed)
            })
        }));
    }
    paths.retain(|p| p.is_file());
    paths.sort();
    paths.dedup();
    Ok(paths)
}
fn executables(binaries: &Path) -> Vec<PathBuf> {
    let mut paths = vec![binaries.join("openpilot-camerad")];
    for name in POLICIES.iter().map(|(_, _, name)| *name).chain([
        "kernel_operations",
        "sensor_lifecycle",
        "isp_lifecycle",
        "camera_lifecycle",
        "frame_state",
        "stress_trace",
        "stress_parse_trace",
    ]) {
        paths.push(binaries.join("examples").join(name));
    }
    for name in [
        "requests-reference",
        "packet-reference",
        "bps-data",
        "sensor-data",
        "ae-reference",
        "kernel-reference",
        "sensor-lifecycle-reference",
        "isp-lifecycle-reference",
        "camera-lifecycle-reference",
        "state-reference",
        "stress-reference",
        "runtime-fixture",
    ] {
        paths.push(binaries.join(format!("generate-camerad-{name}")));
    }
    for name in POLICIES.iter().map(|(_, name, _)| *name).chain([
        "kernel",
        "sensor-lifecycle",
        "isp-lifecycle",
        "camera-lifecycle",
        "state",
        "runtime-stress",
        "runtime",
    ]) {
        paths.push(binaries.join(format!("check-camerad-{name}")));
    }
    paths
}
fn hash(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
/// Preserve source preflight ordering: reserve, create output, hash sources and
/// every required executable, then publish the receipt before executing commands.
pub fn prepare(
    options: &Options,
    argv: &[OsString],
    space: &mut dyn FnMut(&Path, u64) -> Result<()>,
) -> Result<()> {
    space(&options.output, 2 * 1024 * 1024 * 1024)?;
    create_new_tree(&options.output)?;
    let mut sources = vec![];
    for path in source_paths(&options.root)? {
        sources.push(format!(
            "{}: {}",
            python_fs::json_string(path.strip_prefix(&options.root)?.as_os_str())?,
            serde_json::to_string(&hash(&path)?)?
        ));
    }
    let mut binaries = vec![];
    for path in executables(&options.binaries) {
        binaries.push(format!(
            "{}: {}",
            python_fs::json_string(path.as_os_str())?,
            serde_json::to_string(&hash(&path)?)?
        ));
    }
    let args = argv
        .iter()
        .map(|v| python_fs::json_string(v))
        .collect::<Result<Vec<_>>>()?;
    let receipt = crate::control_tools_policy_check::json::parse_text(&format!(
        "{{\"argv\":[{}],\"source_sha256\":{{{}}},\"binary_sha256\":{{{}}}}}",
        args.join(","),
        sources.join(","),
        binaries.join(",")
    ))?;
    let pretty = crate::control_tools_policy_check::json::pretty_with_limit(&receipt, 0, 996)?;
    fs::write(options.output.join("invocation.json"), pretty + "\n")?;
    Ok(())
}

fn require_space(path: &Path, growth: u64) -> Result<()> {
    let space = crate::disk_space::inspect(path, growth)?;
    println!(
        "{{\"free_bytes\": {}, \"required_bytes\": {}, \"growth_bytes\": {}}}",
        space.free_bytes, space.required_bytes, space.growth_bytes
    );
    std::io::stdout().flush()?;
    space.require()
}
pub fn cli() -> Result<()> {
    use crate::argparse_scan::{scan_os, OptionSpec};
    let argv = std::env::args_os().collect::<Vec<_>>();
    let parsed = scan_os(
        &argv[1..],
        &[
            OptionSpec::value("--binaries", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--compiler", false),
        ],
    )?;
    if parsed.help {
        println!("usage: check-camerad-ci --binaries DIRECTORY --output DIRECTORY [--compiler COMPILER]\nRebuild camera source oracles and exercise native host driver fixtures");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let options = Options {
        root: crate::path_resolve::resolve_non_strict(&root)?,
        output: crate::path_resolve::resolve_non_strict(Path::new(&parsed.values["--output"]))?,
        binaries: crate::path_resolve::resolve_non_strict(Path::new(&parsed.values["--binaries"]))?,
        compiler: parsed
            .values
            .get("--compiler")
            .cloned()
            .unwrap_or_else(|| "clang++-18".into()),
    };
    prepare(&options, &argv, &mut require_space)?;
    execute(
        &options,
        &mut crate::qa_process::NativeRunner,
        &mut require_space,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )?;
    fs::write(options.output.join("result.json"),format!("{{\"status\": \"PASS\", \"runtime_cases\": [{}], \"scope\": \"host source comparisons and native driver fixtures; no hardware or full startup acceptance\"}}\n",CASES.iter().map(|v|format!("\"{v}\"")).collect::<Vec<_>>().join(", ")))?;
    Ok(())
}
