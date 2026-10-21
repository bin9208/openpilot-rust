//! Host-only Panda QA orchestration. The original phase checkers remain explicit children.
//! Phase order is independently observed in Actions run 39542206911, artifact 12319964928.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    disk_space,
    path_resolve::resolve_non_strict,
    qa_process::{CaptureError, Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use flate2::read::GzDecoder;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

const MIB: u64 = 1024 * 1024;
const SCOPE: &str = "host source comparisons and owned native fixtures; no physical USB/SPI, full startup or device acceptance";
pub const HELP: &str = "Usage: check-pandad-ci --binaries BINARIES --build-messages MESSAGES --output OUTPUT [--python PYTHON]\n\nRebuild original Panda references and require native USB/SPI and supervisor comparisons.\n";

#[derive(Debug, Clone)]
pub struct Options {
    pub root: PathBuf,
    pub binaries: PathBuf,
    pub build_messages: PathBuf,
    pub output: PathBuf,
    pub python: OsString,
}
#[derive(Debug)]
pub enum Command {
    Help,
    Run(Options),
}
pub fn parse(args: &[OsString], root: &Path) -> Result<Command> {
    let parsed = scan_os(
        args,
        &[
            OptionSpec::value("--binaries", true),
            OptionSpec::value("--build-messages", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--python", false),
        ],
    )?;
    if parsed.help {
        return Ok(Command::Help);
    }
    // argparse handles help before evaluating an otherwise unused raw path.
    // Actual invocation receipts cannot encode such paths losslessly yet.
    for arg in args {
        if arg.to_str().is_none() {
            return Err(crate::argparse_scan::ArgumentError(
                "Panda CI argument cannot be represented losslessly in JSON".into(),
            )
            .into());
        }
    }
    Ok(Command::Run(Options {
        root: root.canonicalize()?,
        binaries: resolve_non_strict(Path::new(&parsed.values["--binaries"]))?,
        build_messages: PathBuf::from(parsed.values["--build-messages"].clone()),
        output: resolve_non_strict(Path::new(&parsed.values["--output"]))?,
        python: parsed
            .values
            .get("--python")
            .cloned()
            .or_else(|| std::env::var_os("PANDA_CI_PYTHON"))
            .unwrap_or_else(|| OsString::from("python3")),
    }))
}
fn utf8(value: &OsStr) -> Result<&str> {
    value
        .to_str()
        .ok_or_else(|| "Panda CI argument cannot be represented losslessly in JSON".into())
}
fn sha(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let len = file.read(&mut buf)?;
        if len == 0 {
            break;
        }
        digest.update(&buf[..len]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn save(path: &Path, value: &Value) -> Result<()> {
    let mut file = File::create(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    Ok(())
}
fn space_receipt(reservation: &disk_space::Space, stdout: &mut dyn Write) -> Result<()> {
    // The original require_space publishes its observation before rejecting a
    // low-capacity filesystem. Keep the attempted reservation in failed output.
    writeln!(
        stdout,
        "{{\"free_bytes\": {}, \"required_bytes\": {}, \"growth_bytes\": {}}}",
        reservation.free_bytes, reservation.required_bytes, reservation.growth_bytes
    )?;
    stdout.flush()?;
    reservation.require()?;
    Ok(())
}
fn space(path: &Path, mib: u64, stdout: &mut dyn Write) -> Result<()> {
    let reservation = disk_space::inspect(path, mib * MIB)?;
    space_receipt(&reservation, stdout)
}
fn os(path: &Path) -> OsString {
    path.as_os_str().to_owned()
}
fn word(value: &str) -> OsString {
    value.into()
}
fn arg_path(flag: &str, path: &Path) -> [OsString; 2] {
    [word(flag), os(path)]
}
fn args(program: &Path, rest: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    std::iter::once(os(program)).chain(rest).collect()
}
fn files(
    root: &Path,
    dir: &Path,
    recursive: bool,
    suffix: &str,
    out: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let link = fs::symlink_metadata(&path)?;
        // Python Path.is_file() follows a regular-file link. Broken links do
        // not enter its hash map; linked directories are not traversed here.
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error)
                if error.kind() == io::ErrorKind::NotFound && link.file_type().is_symlink() =>
            {
                continue
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.is_dir() {
            if recursive && !link.file_type().is_symlink() {
                files(root, &path, true, suffix, out)?;
            }
        } else if metadata.is_file()
            && path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.ends_with(suffix))
        {
            out.insert(path.strip_prefix(root)?.to_owned());
        }
    }
    Ok(())
}
/// Force NativeRunner through Runner's bounded capture implementation. Its
/// optimized run_to_files override streams without applying stream limits.
struct Bounded<'a, R: Runner>(&'a mut R);
impl<R: Runner> Runner for Bounded<'_, R> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let captured = self.0.run(spec)?;
        for (stream, bytes, limit) in [
            ("stdout", &captured.stdout, spec.stdout_limit),
            ("stderr", &captured.stderr, spec.stderr_limit),
        ] {
            if bytes.len() > limit {
                return Err(CaptureError::Limit { stream, limit }.into());
            }
        }
        Ok(captured)
    }
}
fn source_paths(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut paths = BTreeSet::new();
    let tools = root.join("rust/tools");
    files(root, &tools, false, "", &mut paths)?;
    paths.retain(|p| {
        p.file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|n| n.contains("pandad"))
    });
    let src = root.join("rust/crates/dev-tools/src");
    let mut others = BTreeSet::new();
    files(root, &src, false, ".rs", &mut others)?;
    paths.extend(others.into_iter().filter(|p| {
        p.file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|n| n.contains("panda"))
    }));
    let mut bins = BTreeSet::new();
    files(root, &src.join("bin"), false, ".rs", &mut bins)?;
    paths.extend(bins.into_iter().filter(|p| {
        p.file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|n| n.starts_with("build_pandad_"))
    }));
    for name in [
        "json11_stage.rs",
        "services_header.rs",
        "qa_process.rs",
        "path_resolve.rs",
        "bin/stage_json11.rs",
        "bin/check_pandad_safety_padding.rs",
        "pandad_safety_padding_check/ownership.rs",
    ] {
        paths.insert(PathBuf::from("rust/crates/dev-tools/src").join(name));
    }
    for name in [
        "rust/crates/dev-tools/Cargo.toml",
        "rust/crates/disk-space/src/lib.rs",
        "rust/crates/messaging/src/services_generated.rs",
        "rust/Cargo.lock",
        "uv.lock",
    ] {
        paths.insert(name.into());
    }
    for name in ["pandad", "panda-usb", "panda-spi", "panda-spi-linux"] {
        files(
            root,
            &root.join("rust/crates").join(name),
            true,
            ".rs",
            &mut paths,
        )?;
    }
    Ok(paths
        .into_iter()
        .filter(|p| root.join(p).is_file())
        .collect())
}
fn executables(bin: &Path) -> Vec<PathBuf> {
    let mut out: Vec<_> = [
        "openpilot-pandad",
        "openpilot-pandad-supervisor",
        "openpilot-process-child",
        "openpilot-logmessaged",
        "stage-json11",
        "build-pandad-protocol-source",
        "build-pandad-peripheral-source",
        "build-pandad-spi-source",
        "build-pandad-usb-source",
        "build-pandad-runtime-fixture",
        "build-pandad-runtime-source",
        "build-pandad-firmware-spi-fixture",
        "check-pandad-safety-padding",
    ]
    .iter()
    .map(|s| bin.join(s))
    .collect();
    out.extend(
        [
            "panda_protocol",
            "panda_safety",
            "device",
            "state",
            "can_io",
            "peripheral",
            "firmware",
            "firmware_client",
            "firmware_spi",
            "supervisor",
            "supervisor_native",
            "dfu_recovery",
            "usb_policy",
            "spi_policy",
            "usb_fixture",
            "raw_fixture",
            "spi_fixture",
        ]
        .iter()
        .map(|s| bin.join("examples").join(s)),
    );
    out
}
pub fn zmq_output(messages: &Path) -> Result<PathBuf> {
    let mut outputs = BTreeSet::new();
    for line in fs::read_to_string(messages)?.lines() {
        let record: Value = serde_json::from_str(line)?;
        if record.get("reason").and_then(Value::as_str) == Some("build-script-executed")
            && record
                .get("package_id")
                .and_then(Value::as_str)
                .is_some_and(|s| s.contains("zmq-sys"))
        {
            outputs.insert(
                PathBuf::from(record["out_dir"].as_str().ok_or("zmq output missing")?)
                    .canonicalize()?,
            );
        }
    }
    if outputs.len() != 1 {
        return Err(format!("expected one executed zmq-sys build output, got {outputs:?}").into());
    }
    let output = outputs.pop_first().ok_or("missing zmq output")?;
    for name in ["source/include/zmq.h", "lib/libzmq.a"] {
        if !output.join(name).is_file() {
            return Err(format!("missing zmq dependency: {}", output.join(name).display()).into());
        }
    }
    Ok(output)
}
fn invocation(options: &Options, zmq: &Path) -> Result<Value> {
    let source_sha256 = source_paths(&options.root)?
        .into_iter()
        .map(|p| Ok((utf8(p.as_os_str())?.to_owned(), sha(&options.root.join(p))?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let binary_sha256 = executables(&options.binaries)
        .into_iter()
        .map(|p| Ok((utf8(p.as_os_str())?.to_owned(), sha(&p)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let argv = std::env::args_os()
        .map(|arg| Ok(utf8(&arg)?.to_owned()))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({"argv": argv, "source_sha256": source_sha256,
        "binary_sha256": binary_sha256, "zmq_output": zmq, "build_messages_sha256": sha(&options.build_messages)?}))
}
struct Controller<'a, R: Runner> {
    options: &'a Options,
    runner: &'a mut R,
    commands: PathBuf,
    stdout: &'a mut dyn Write,
    stderr: &'a mut dyn Write,
}
impl<R: Runner> Controller<'_, R> {
    fn output(&self, name: &str) -> PathBuf {
        self.options.output.join(name)
    }
    fn bin(&self, name: &str) -> PathBuf {
        self.options.binaries.join(name)
    }
    fn example(&self, name: &str) -> PathBuf {
        self.options.binaries.join("examples").join(name)
    }
    fn command(&mut self, name: &str, argv: Vec<OsString>, growth: u64) -> Result<Vec<u8>> {
        space(&self.options.output, growth, &mut *self.stdout)?;
        fs::create_dir_all(&self.commands)?;
        let recorded = argv
            .iter()
            .map(|arg| Ok(utf8(arg)?.to_owned()))
            .collect::<Result<Vec<_>>>()?;
        let command = json!({"argv": recorded, "cwd": utf8(self.options.root.as_os_str())?});
        save(
            &self.commands.join(format!("{name}.command.json")),
            &command,
        )?;
        let mut spec = CommandSpec::new_os(argv, Duration::from_secs(600));
        spec.cwd = Some(self.options.root.clone());
        let stdout = self.commands.join(format!("{name}.stdout"));
        let stderr = self.commands.join(format!("{name}.stderr"));
        let captured = match Bounded(&mut *self.runner).run_to_files(&spec, &stdout, &stderr) {
            Ok(captured) => captured,
            Err(error) => {
                save(
                    &self.commands.join(format!("{name}.failure.json")),
                    &json!({"error": error.to_string(), "phase": name}),
                )?;
                if let Ok(bytes) = fs::read(&stdout) {
                    self.stderr.write_all(&bytes)?;
                }
                if let Ok(bytes) = fs::read(&stderr) {
                    self.stderr.write_all(&bytes)?;
                }
                self.stderr.flush()?;
                return Err(error);
            }
        };
        let output = fs::read(&stdout)?;
        let errors = fs::read(&stderr)?;
        for (stream, bytes) in [("stdout", &output), ("stderr", &errors)] {
            if let Err(error) = std::str::from_utf8(bytes) {
                let detail = format!("Panda phase {name} {stream} is not UTF-8: {error}");
                save(
                    &self.commands.join(format!("{name}.failure.json")),
                    &json!({"error": detail, "phase": name, "stream": stream}),
                )?;
                return Err(detail.into());
            }
        }
        save(
            &self.commands.join(format!("{name}.exit.json")),
            &json!({"returncode": captured.code}),
        )?;
        if captured.code != 0 {
            self.stderr.write_all(&output)?;
            self.stderr.write_all(&errors)?;
            self.stderr.flush()?;
            return Err(format!("Panda phase {name} exited {}", captured.code).into());
        }
        writeln!(self.stdout, "PASS {name}")?;
        self.stdout.flush()?;
        Ok(output)
    }
    fn python(&mut self, name: &str, script: &str, rest: Vec<OsString>) -> Result<()> {
        let argv = std::iter::once(self.options.python.clone())
            .chain(std::iter::once(os(&self
                .options
                .root
                .join("rust/tools")
                .join(script))))
            .chain(rest)
            .collect();
        self.command(name, argv, 512)?;
        Ok(())
    }
    fn check(&mut self, name: &str, rest: Vec<OsString>, script: Option<&str>) -> Result<()> {
        let script = script
            .map(str::to_owned)
            .unwrap_or_else(|| format!("check_pandad_{name}.py"));
        let mut rest = rest;
        rest.extend(arg_path("--output", &self.output(name)));
        self.python(&format!("check-{name}"), &script, rest)
    }
    fn json11(&mut self) -> Result<PathBuf> {
        let stdout = self.command(
            "stage-json11",
            args(
                &self.bin("stage-json11"),
                arg_path("--root", &self.options.root)
                    .into_iter()
                    .chain(arg_path("--output", &self.output("json11-dependency"))),
            ),
            256,
        )?;
        let staged: Value = serde_json::from_slice(&stdout)?;
        save(
            &self.output("json11-dependency.json"),
            staged.get("dependency").ok_or("missing json11 receipt")?,
        )?;
        Ok(PathBuf::from(
            staged["prefix"].as_str().ok_or("missing json11 prefix")?,
        ))
    }
    fn spidev(&mut self) -> Result<PathBuf> {
        let lock: toml::Value =
            toml::from_str(&fs::read_to_string(self.options.root.join("uv.lock"))?)?;
        let package = lock["package"]
            .as_array()
            .ok_or("uv.lock missing package")?
            .iter()
            .find(|v| v["name"].as_str() == Some("spidev"))
            .ok_or("uv.lock missing spidev")?;
        let sdist = &package["sdist"];
        let url = sdist["url"].as_str().ok_or("missing spidev URL")?;
        let expected = sdist["hash"].as_str().ok_or("missing spidev hash")?;
        let basename = url
            .rsplit('/')
            .next()
            .ok_or("missing spidev archive name")?;
        if basename.is_empty() || basename == "." || basename == ".." || basename.contains('/') {
            return Err("invalid spidev archive name".into());
        }
        let archive = self.output(basename);
        self.command(
            "download-spidev",
            vec![
                word("curl"),
                word("--retry"),
                word("3"),
                word("-fsSL"),
                word(url),
                word("-o"),
                os(&archive),
            ],
            64,
        )?;
        let actual = format!("sha256:{}", sha(&archive)?);
        if actual != expected {
            return Err(format!("spidev source hash mismatch: {actual} != {expected}").into());
        }
        if let Some(size) = sdist.get("size").and_then(toml::Value::as_integer) {
            if fs::metadata(&archive)?.len() != u64::try_from(size)? {
                return Err("spidev archive size differs from uv.lock".into());
            }
        }
        space(&self.options.output, 64, &mut *self.stdout)?;
        let extracted = self.output("spidev-source");
        fs::create_dir(&extracted)?;
        let archive_file = File::open(&archive)?;
        let reader: Box<dyn Read> = if basename.ends_with(".gz") || basename.ends_with(".tgz") {
            Box::new(GzDecoder::new(archive_file))
        } else {
            Box::new(archive_file)
        };
        let mut tar = tar::Archive::new(reader);
        let mut extracted_bytes = 0u64;
        for entry in tar.entries()? {
            let mut entry = entry?;
            let rel = entry.path()?.into_owned();
            if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
                return Err(format!("unsafe spidev archive member: {}", rel.display()).into());
            }
            if !entry.header().entry_type().is_file() && !entry.header().entry_type().is_dir() {
                return Err(format!("unsupported spidev archive member: {}", rel.display()).into());
            }
            extracted_bytes = extracted_bytes
                .checked_add(entry.size())
                .ok_or("spidev size overflow")?;
            if extracted_bytes > 64 * MIB {
                return Err("spidev extraction exceeds 64 MiB".into());
            }
            if !entry.unpack_in(&extracted)? {
                return Err(format!("spidev archive escaped output: {}", rel.display()).into());
            }
        }
        let mut modules = BTreeSet::new();
        for dir in fs::read_dir(&extracted)? {
            let dir = dir?.path();
            if dir.is_dir() && dir.join("spidev_module.c").is_file() {
                modules.insert(dir);
            }
        }
        if modules.len() != 1 {
            return Err(format!("expected one original spidev module, got {modules:?}").into());
        }
        save(
            &self.output("spidev-dependency.json"),
            &json!({"version": package["version"], "url": url, "hash": actual}),
        )?;
        Ok(modules.pop_first().ok_or("missing spidev source")?)
    }
    fn python_metadata(&mut self) -> Result<(String, String, PathBuf)> {
        // Metadata only: no Python orchestration or checker body is imported.
        let program = "import json,sysconfig,pathlib,usb1; print(json.dumps({'include':sysconfig.get_paths()['include'],'suffix':sysconfig.get_config_var('EXT_SUFFIX'),'usb1':str(pathlib.Path(usb1.__file__).resolve())}))";
        // The native metadata probe reserves one additional 64 MiB. It is a
        // deliberate 42nd receipt, distinct from the original 41 checks.
        space(&self.options.output, 64, &mut *self.stdout)?;
        let mut spec = CommandSpec::new_os(
            vec![
                self.options.python.clone(),
                word("-P"),
                word("-c"),
                word(program),
            ],
            Duration::from_secs(60),
        );
        spec.cwd = Some(self.options.root.clone());
        let observed = self.runner.run(&spec)?;
        let recorded = spec
            .effective_argv()
            .iter()
            .map(|arg| Ok(utf8(arg)?.to_owned()))
            .collect::<Result<Vec<_>>>()?;
        let observed_stdout = String::from_utf8(observed.stdout)?;
        let observed_stderr = String::from_utf8(observed.stderr)?;
        save(
            &self.output("python-dependency.json"),
            &json!({
                "argv": recorded,
                "exit": observed.code,
                "stdout": observed_stdout,
                "stderr": observed_stderr
            }),
        )?;
        if observed.code != 0 {
            return Err(format!("Python ABI/usb1 discovery exited {}", observed.code).into());
        }
        let meta: Value = serde_json::from_str(&observed_stdout)?;
        Ok((
            meta["include"]
                .as_str()
                .ok_or("missing Python include")?
                .to_owned(),
            meta["suffix"]
                .as_str()
                .ok_or("missing Python extension suffix")?
                .to_owned(),
            PathBuf::from(meta["usb1"].as_str().ok_or("missing usb1 binding")?),
        ))
    }
    fn execute(&mut self, zmq: &Path) -> Result<()> {
        space(&self.options.output, 256, &mut *self.stdout)?;
        let json11 = self.json11()?;
        let common = arg_path("--json11-prefix", &json11);
        for mode in ["protocol", "safety", "device", "state", "can-io"] {
            let mut build = args(
                &self.bin("build-pandad-protocol-source"),
                arg_path("--root", &self.options.root)
                    .into_iter()
                    .chain(arg_path("--output", &self.output(&format!("{mode}-src"))))
                    .chain(arg_path("--capnp-prefix", Path::new("/usr")))
                    .chain(common.clone())
                    .chain([word("--sanitize")]),
            );
            if mode != "protocol" {
                build.push(word(&format!("--{mode}")));
            }
            self.command(&format!("build-{mode}"), build, 512)?;
            let name = mode.replace('-', "_");
            let source = self.output(&format!("{mode}-src/pandad-protocol-source"));
            let (flag, example) = if mode == "protocol" || mode == "safety" {
                ("--binary", format!("panda_{mode}"))
            } else {
                ("--native", name.clone())
            };
            let check = vec![
                word("--source"),
                os(&source),
                word(flag),
                os(&self.example(&example)),
            ];
            self.check(&name, check.clone(), None)?;
            if mode == "safety" {
                self.command(
                    "check-safety_padding",
                    args(
                        &self.bin("check-pandad-safety-padding"),
                        check
                            .into_iter()
                            .chain(arg_path("--output", &self.output("safety_padding"))),
                    ),
                    512,
                )?;
            }
        }
        for (name, example) in [("peripheral", "peripheral"), ("spi", "spi_fixture")] {
            self.command(
                &format!("build-{name}"),
                args(
                    &self.bin(&format!("build-pandad-{name}-source")),
                    arg_path("--root", &self.options.root)
                        .into_iter()
                        .chain(arg_path("--output", &self.output(&format!("{name}-src"))))
                        .chain(common.clone())
                        .chain([word("--sanitize")]),
                ),
                512,
            )?;
            self.check(
                name,
                vec![
                    word("--source"),
                    os(&self.output(&format!("{name}-src/pandad-{name}-source"))),
                    word("--native"),
                    os(&self.example(example)),
                ],
                None,
            )?;
        }
        self.command(
            "build-usb",
            args(
                &self.bin("build-pandad-usb-source"),
                arg_path("--root", &self.options.root)
                    .into_iter()
                    .chain(arg_path("--output", &self.output("usb-src")))
                    .chain(common.clone())
                    .chain(arg_path("--libusb-include", Path::new("/usr/include"))),
            ),
            512,
        )?;
        let usb = self.output("usb-src/libpanda_usb_fixture.so");
        self.check(
            "usb",
            vec![
                word("--source"),
                os(&self.output("usb-src/pandad-usb-source")),
                word("--binary"),
                os(&self.example("usb_fixture")),
                word("--fixture"),
                os(&usb),
            ],
            None,
        )?;
        for (name, example) in [
            ("firmware", "firmware"),
            ("client", "firmware_client"),
            ("firmware_spi", "firmware_spi"),
            ("supervisor", "supervisor"),
        ] {
            self.check(
                name,
                vec![word("--binary"), os(&self.example(example))],
                None,
            )?;
        }
        self.check(
            "usb_policy",
            vec![
                word("--binary"),
                os(&self.example("usb_policy")),
                word("--fixture"),
                os(&usb),
            ],
            None,
        )?;
        let (include, suffix, binding) = self.python_metadata()?;
        self.check(
            "usb_raw",
            vec![
                word("--binary"),
                os(&self.example("raw_fixture")),
                word("--fixture"),
                os(&usb),
                word("--binding"),
                os(&binding),
            ],
            None,
        )?;
        let spidev = self.spidev()?;
        self.command(
            "build-spidev",
            args(
                &self.bin("build-pandad-firmware-spi-fixture"),
                arg_path("--root", &self.options.root)
                    .into_iter()
                    .chain(arg_path("--output", &self.output("spidev-src")))
                    .chain(common.clone())
                    .chain(arg_path("--spidev-source", &spidev))
                    .chain([
                        word("--python-include"),
                        word(&include),
                        word("--extension-suffix"),
                        word(&suffix),
                    ]),
            ),
            512,
        )?;
        self.check(
            "spidev",
            vec![
                word("--native"),
                os(&self.example("spi_policy")),
                word("--library"),
                os(&self.output("spidev-src/libpandad_firmware_spi.so")),
            ],
            None,
        )?;
        self.command(
            "build-runtime-fixture",
            args(
                &self.bin("build-pandad-runtime-fixture"),
                arg_path("--root", &self.options.root)
                    .into_iter()
                    .chain(arg_path("--output", &self.output("runtime-fixture")))
                    .chain(common.clone())
                    .chain(arg_path("--libusb-include", Path::new("/usr/include"))),
            ),
            512,
        )?;
        let runtime_usb = self.output("runtime-fixture/libusb-1.0.so.0");
        self.command(
            "build-runtime-source",
            args(
                &self.bin("build-pandad-runtime-source"),
                arg_path("--root", &self.options.root)
                    .into_iter()
                    .chain(arg_path("--output", &self.output("runtime-src")))
                    .chain(arg_path("--generated-root", &self.output("protocol-src")))
                    .chain(arg_path("--capnp-prefix", Path::new("/usr")))
                    .chain(common.clone())
                    .chain(arg_path("--libusb-include", Path::new("/usr/include")))
                    .chain(arg_path("--zmq-root", zmq))
                    .chain(arg_path("--fixture", &runtime_usb))
                    .chain([word("--sanitize")]),
            ),
            512,
        )?;
        for (name, binary, original) in [
            ("source", self.output("runtime-src/pandad-source"), true),
            ("native", self.bin("openpilot-pandad"), false),
        ] {
            let mut check = vec![
                word("--binary"),
                os(&binary),
                word("--collector"),
                os(&self.bin("openpilot-logmessaged")),
                word("--fixture"),
                os(&runtime_usb),
            ];
            if original {
                check.push(word("--original"));
            }
            self.check(
                &format!("runtime-{name}"),
                check,
                Some("check_pandad_runtime.py"),
            )?;
        }
        self.python(
            "compare-runtime",
            "compare_pandad_runtime.py",
            vec![
                word("--source"),
                os(&self.output("runtime-source")),
                word("--native"),
                os(&self.output("runtime-native")),
                word("--output"),
                os(&self.output("runtime-comparison.json")),
            ],
        )?;
        let child = self.output("supervisor-child");
        self.command(
            "build-supervisor-child",
            vec![
                word("clang++"),
                word("-std=c++17"),
                word("-O1"),
                word("-g"),
                os(&self
                    .options
                    .root
                    .join("rust/tools/pandad_supervisor_child.cc")),
                word("-o"),
                os(&child),
            ],
            64,
        )?;
        for name in ["native_supervisor", "supervisor_signals"] {
            self.check(
                name,
                vec![
                    word("--binary"),
                    os(&self.example("supervisor_native")),
                    word("--launcher"),
                    os(&self.bin("openpilot-process-child")),
                    word("--library"),
                    os(&usb),
                    word("--child"),
                    os(&child),
                ],
                None,
            )?;
        }
        self.check(
            "dfu_recovery",
            vec![
                word("--binary"),
                os(&self.example("dfu_recovery")),
                word("--fixture"),
                os(&usb),
            ],
            None,
        )?;
        for (name, default) in [("explicit", false), ("default", true)] {
            let mut check = vec![
                word("--supervisor"),
                os(&self.bin("openpilot-pandad-supervisor")),
                word("--core"),
                os(&self.bin("openpilot-pandad")),
                word("--launcher"),
                os(&self.bin("openpilot-process-child")),
                word("--collector"),
                os(&self.bin("openpilot-logmessaged")),
                word("--fixture"),
                os(&runtime_usb),
                word("--cli"),
            ];
            if default {
                check.push(word("--default-firmware"));
            }
            self.check(
                &format!("composed-{name}"),
                check,
                Some("check_pandad_composed.py"),
            )?;
        }
        Ok(())
    }
}
/// One uninterrupted original-compatible phase sequence. Existing children are explicit.
pub fn run_with_output<R: Runner>(
    options: &Options,
    runner: &mut R,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<Value> {
    for value in [
        options.root.as_os_str(),
        options.binaries.as_os_str(),
        options.build_messages.as_os_str(),
        options.output.as_os_str(),
        options.python.as_os_str(),
    ] {
        let _ = utf8(value)?;
    }
    space(&options.output, 2048, stdout)?;
    if options.output.exists() {
        return Err(format!("Panda output already exists: {}", options.output.display()).into());
    }
    fs::create_dir_all(&options.output)?;
    let zmq = zmq_output(&options.build_messages)?;
    save(
        &options.output.join("invocation.json"),
        &invocation(options, &zmq)?,
    )?;
    let mut controller = Controller {
        options,
        runner,
        commands: options.output.join("commands"),
        stdout,
        stderr,
    };
    controller.execute(&zmq)?;
    let result = json!({"status": "PASS", "scope": SCOPE});
    save(&options.output.join("result.json"), &result)?;
    Ok(result)
}
pub fn run_with<R: Runner>(options: &Options, runner: &mut R) -> Result<Value> {
    run_with_output(
        options,
        runner,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )
}
pub fn run(options: &Options) -> Result<Value> {
    run_with(options, &mut NativeRunner)
}
/// The bin prints disk reservations and ordered PASS lines; result.json is final.
pub fn cli() -> Result<()> {
    let root = std::env::var_os("OPENPILOT_RUST_ROOT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        PathBuf::from,
    );
    match parse(&std::env::args_os().skip(1).collect::<Vec<_>>(), &root)? {
        Command::Help => io::stdout()
            .lock()
            .write_all(HELP.as_bytes())
            .map_err(Into::into),
        Command::Run(options) => {
            run(&options)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod space_receipt_tests {
    use super::*;

    #[test]
    fn rejected_reservation_is_published_before_failure() {
        let growth = 64 * MIB;
        let required = disk_space::RESERVE_BYTES + growth;
        let reservation = disk_space::Space::new(required - 1, growth).unwrap();
        let mut stdout = Vec::new();
        assert!(space_receipt(&reservation, &mut stdout).is_err());
        let observed: Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(observed["free_bytes"], required - 1);
        assert_eq!(observed["required_bytes"], required);
        assert_eq!(observed["growth_bytes"], growth);
        assert!(stdout.ends_with(b"\n"));
    }

    #[test]
    fn failed_receipt_write_does_not_accept_reservation() {
        struct Refuse;
        impl Write for Refuse {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "receipt sink failed",
                ))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let reservation = disk_space::Space::new(disk_space::RESERVE_BYTES + MIB, MIB).unwrap();
        assert!(space_receipt(&reservation, &mut Refuse).is_err());
    }
}
