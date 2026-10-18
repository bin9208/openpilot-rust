//! Encoder CI build provenance and all three independent host comparison stages.
//! External programs are native executables; no Python bootstrap or oracle wrapper.
use crate::{
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
const GIB: u64 = 1024 * 1024 * 1024;
/// Relative sources that are required even if their containing directory is empty.
const REQUIRED: &[&str] = &[
    "rust/Cargo.lock",
    "uv.lock",
    "rust/crates/dev-tools/Cargo.toml",
    "rust/crates/dev-tools/src/encoder_source_build.rs",
    "rust/crates/dev-tools/src/encoder_codecs_check.rs",
    "rust/crates/dev-tools/src/encoder_v4l_check.rs",
    "rust/crates/dev-tools/src/encoder_runtime_native.rs",
    "rust/crates/dev-tools/src/encoder_runtime_wire.rs",
    "rust/crates/dev-tools/src/encoder_runtime_check.rs",
    "rust/crates/dev-tools/src/encoder_runtime_exercise.rs",
    "rust/crates/dev-tools/src/encoder_runtime_process.rs",
    "rust/crates/dev-tools/src/encoder_ci_check.rs",
    "rust/crates/dev-tools/src/qa_process.rs",
    "rust/crates/dev-tools/src/services_header.rs",
    "rust/crates/dev-tools/src/path_resolve.rs",
    "rust/crates/dev-tools/src/bin/build_encoder_source.rs",
    "rust/crates/dev-tools/src/bin/check_encoder_codecs.rs",
    "rust/crates/dev-tools/src/bin/check_encoder_v4l.rs",
    "rust/crates/dev-tools/src/bin/check_encoder_runtime.rs",
    "rust/crates/dev-tools/src/bin/check_encoder_ci.rs",
    "rust/crates/messaging/src/services_generated.rs",
    "openpilot/cereal/log.capnp",
    "openpilot/cereal/deprecated.capnp",
    "openpilot/cereal/custom.capnp",
    "openpilot/cereal/include/c++.capnp",
    "opendbc_repo/opendbc/car/car.capnp",
];
#[derive(Debug)]
pub struct Options {
    pub root: PathBuf,
    pub binaries: PathBuf,
    pub build_messages: PathBuf,
    pub output: PathBuf,
    pub native: PathBuf,
    pub capnp: PathBuf,
    pub json11_prefix: Option<PathBuf>,
    pub json11_stager: Option<PathBuf>,
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn string(value: &JsonValue) -> Result<String> {
    value.to_utf8().ok_or_else(|| "expected JSON text".into())
}
fn get(value: &JsonValue, name: &str) -> Result<JsonValue> {
    value.get(name).ok_or_else(|| format!("'{name}'").into())
}
fn dump(path: &Path, encoded: &str) -> Result<()> {
    let value = json::parse_text(encoded)?;
    fs::write(path, json::pretty(&value, 0)? + "\n")?;
    Ok(())
}
fn os(value: impl AsRef<OsStr>) -> OsString {
    value.as_ref().to_owned()
}
fn argv_json(argv: &[OsString]) -> Result<String> {
    Ok(format!(
        "[{}]",
        argv.iter()
            .map(|v| python_fs::json_string(v))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))
}
/// Only current executed build outputs are admitted; repeated identical paths
/// are deduplicated after resolving symlinks, as in the original set of Paths.
pub fn dependency_outputs(messages: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let source = text(fs::read(messages)?)?;
    // str.splitlines() recognizes these separators in addition to ordinary LF.
    let mut lines = source
        .split([
            '\n', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let entries = lines
        .into_iter()
        .map(json::parse_text)
        .collect::<Result<Vec<_>>>()?;
    for entry in &entries {
        if !matches!(entry.view(), JsonView::Object(_)) {
            return Err("Cargo record is not an object".into());
        }
    }
    let finished = entries
        .iter()
        .filter(|v| v.get("reason").and_then(|v| v.to_utf8()).as_deref() == Some("build-finished"))
        .collect::<Vec<_>>();
    if finished.len() != 1
        || !finished[0]
            .get("success")
            .is_some_and(|v| matches!(v.view(), JsonView::Bool(true)))
    {
        return Err("expected one successful completed Cargo build".into());
    }
    let mut result = BTreeMap::new();
    for (name, key, suffix, files) in [
        ("openpilot-jpeg", "jpeg", "jpeg", ["libjpeg.a", "jconfig.h"]),
        (
            "zmq-sys",
            "zmq",
            "",
            ["source/include/zmq.h", "lib/libzmq.a"],
        ),
    ] {
        let mut paths = BTreeSet::new();
        for entry in &entries {
            if entry.get("reason").and_then(|v| v.to_utf8()).as_deref()
                != Some("build-script-executed")
            {
                continue;
            }
            let package = string(&get(entry, "package_id")?)?;
            if package.contains(&format!("#{name}@")) {
                paths.insert(resolve_non_strict(Path::new(&string(&get(
                    entry, "out_dir",
                )?)?))?);
            }
        }
        if paths.len() != 1 {
            return Err(format!("expected one executed {name} build output, got {paths:?}").into());
        }
        let output = paths.into_iter().next().unwrap();
        // Python Path / "" retains the base spelling; PathBuf::join("")
        // would append a visible slash to the captured dependency identity.
        let output = if suffix.is_empty() {
            output
        } else {
            output.join(suffix)
        };
        for relative in files {
            let path = output.join(relative);
            if !path.is_file() {
                return Err(path.display().to_string().into());
            }
        }
        result.insert(key.into(), output);
    }
    Ok(result)
}
struct Context<'a> {
    options: &'a Options,
    runner: &'a mut dyn Runner,
    printed: &'a mut dyn Write,
    errors: &'a mut dyn Write,
}
impl Context<'_> {
    /// card_qa.ci.run: invocation first, both text captures and exit receipt even
    /// on failure, then successful PASS or the original combined diagnostic.
    fn command(&mut self, name: &str, argv: Vec<OsString>) -> Result<PathBuf> {
        let directory = self.options.output.join("commands");
        fs::create_dir_all(&directory)?;
        dump(
            &directory.join(format!("{name}.command.json")),
            &format!(
                "{{\"argv\":{},\"cwd\":{}}}",
                argv_json(&argv)?,
                python_fs::json_string(self.options.root.as_os_str())?
            ),
        )?;
        let mut spec = CommandSpec::new_os(argv, Duration::MAX);
        spec.cwd = Some(self.options.root.clone());
        spec.inherit_stdin = true;
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        let result = self.runner.run(&spec)?;
        let stdout = text(result.stdout)?;
        let stderr = text(result.stderr)?;
        let target = directory.join(format!("{name}.stdout"));
        fs::write(&target, &stdout)?;
        fs::write(directory.join(format!("{name}.stderr")), &stderr)?;
        fs::write(
            directory.join(format!("{name}.exit.json")),
            format!("{{\"returncode\": {}}}\n", result.code),
        )?;
        if result.code != 0 {
            writeln!(self.errors, "{stdout}{stderr}")?;
            return Err(format!("encoder command {name} exited {}", result.code).into());
        }
        writeln!(self.printed, "PASS {name}")?;
        self.printed.flush()?;
        Ok(target)
    }
    fn output(&mut self, argv: Vec<OsString>, cwd: Option<PathBuf>) -> Result<String> {
        let mut spec = CommandSpec::new_os(argv, Duration::MAX);
        spec.cwd = cwd;
        spec.inherit_stdin = true;
        spec.inherit_stderr = true;
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        let result = self.runner.run(&spec)?;
        if result.code != 0 {
            return Err(format!("encoder provenance command exited {}", result.code).into());
        }
        text(result.stdout)
    }
}
pub fn json11_dependency(
    options: &Options,
    runner: &mut dyn Runner,
    space: &mut dyn FnMut(&Path, u64) -> Result<()>,
    printed: &mut dyn Write,
    errors: &mut dyn Write,
) -> Result<PathBuf> {
    if let Some(prefix) = &options.json11_prefix {
        return Ok(prefix.clone());
    }
    let stager = options
        .json11_stager
        .as_ref()
        .ok_or("--json11-stager is required unless --json11-prefix is supplied")?;
    space(&options.output, 256 * 1024 * 1024)?;
    let path = Context {
        options,
        runner,
        printed,
        errors,
    }
    .command(
        "stage-json11",
        vec![
            os(resolve_non_strict(stager)?),
            "--root".into(),
            os(&options.root),
            "--output".into(),
            os(options.output.join("json11-dependency")),
        ],
    )?;
    let staged = json::parse_text(&text(fs::read(path)?)?)?;
    // Original writes metadata before trying the prefix: retain that failure phase.
    fs::write(
        options.output.join("json11.json"),
        get(&staged, "dependency")?.to_json()? + "\n",
    )?;
    Ok(PathBuf::from(string(&get(&staged, "prefix")?)?))
}
pub fn build_source(
    options: &Options,
    dependencies: &BTreeMap<String, PathBuf>,
    json11: &Path,
    runner: &mut dyn Runner,
    space: &mut dyn FnMut(&Path, u64) -> Result<()>,
    printed: &mut dyn Write,
    errors: &mut dyn Write,
) -> Result<()> {
    space(&options.output, GIB)?;
    let jpeg = dependencies.get("jpeg").ok_or("'jpeg'")?;
    let zmq = dependencies.get("zmq").ok_or("'zmq'")?;
    Context {
        options,
        runner,
        printed,
        errors,
    }
    .command(
        "source-build",
        vec![
            os(options.binaries.join("build-encoder-source")),
            "--root".into(),
            os(&options.root),
            "--output".into(),
            os(options.output.join("source")),
            "--native-prefix".into(),
            os(options.native.join("usr")),
            "--capnp-prefix".into(),
            os(resolve_non_strict(&options.capnp)?),
            "--json11-prefix".into(),
            os(resolve_non_strict(json11)?),
            "--zmq-include".into(),
            os(zmq.join("source/include")),
            "--jpeg-build".into(),
            os(jpeg),
        ],
    )?;
    Ok(())
}
fn collect(directory: &Path, recursive: bool, paths: &mut BTreeSet<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            paths.insert(path.clone());
        }
        if recursive && entry.file_type()?.is_dir() {
            collect(&path, true, paths)?;
        }
    }
    Ok(())
}
fn source_paths(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut paths = REQUIRED
        .iter()
        .map(|v| root.join(v))
        .collect::<BTreeSet<_>>();
    let mut tools = BTreeSet::new();
    collect(&root.join("rust/tools"), false, &mut tools)?;
    paths.extend(tools.into_iter().filter(|p| {
        p.file_name()
            .is_some_and(|s| s.as_encoded_bytes().windows(7).any(|w| w == b"encoder"))
    }));
    collect(&root.join("rust/tools/encoder_oracle"), false, &mut paths)?;
    for directory in [
        "rust/crates/encoderd",
        "rust/crates/msgq",
        "rust/crates/jpeg",
        "rust/crates/cereal",
        "openpilot/system/loggerd",
        "msgq_repo/msgq",
        "openpilot/cereal/messaging",
    ] {
        collect(&root.join(directory), true, &mut paths)?;
    }
    // Common code/headers consumed by the C++ oracle are part of its identity too.
    for name in ["params", "util", "swaglog"] {
        for suffix in ["cc", "h"] {
            let p = root.join(format!("openpilot/common/{name}.{suffix}"));
            if p.is_file() {
                paths.insert(p);
            }
        }
    }
    // Native codec checkers may gain an auxiliary policy without changing this list.
    for directory in ["rust/crates/dev-tools/src", "rust/crates/dev-tools/src/bin"] {
        let mut files = BTreeSet::new();
        collect(&root.join(directory), false, &mut files)?;
        paths.extend(files.into_iter().filter(|p| {
            p.extension().is_some_and(|v| v == "rs")
                && p.file_name()
                    .is_some_and(|v| v.as_encoded_bytes().windows(7).any(|w| w == b"encoder"))
        }));
    }
    Ok(paths)
}
fn selected(binaries: &Path) -> Vec<PathBuf> {
    [
        "openpilot-encoderd",
        "examples/encoder_codec_trace",
        "examples/encoder_v4l_trace",
        "build-encoder-source",
        "check-encoder-codecs",
        "check-encoder-v4l",
        "check-encoder-runtime",
        "check-encoder-ci",
    ]
    .map(|name| binaries.join(name))
    .into()
}
fn identity(
    options: &Options,
    dependencies: &BTreeMap<String, PathBuf>,
    context: &mut Context<'_>,
) -> Result<()> {
    let commit = context.output(
        vec!["git".into(), "rev-parse".into(), "HEAD".into()],
        Some(options.root.clone()),
    )?;
    let sources = source_paths(&options.root)?
        .into_iter()
        .map(|p| {
            Ok(format!(
                "{}:{}",
                python_fs::json_string(p.strip_prefix(&options.root)?.as_os_str())?,
                serde_json::to_string(&digest(&p)?)?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let executables = selected(&options.binaries)
        .into_iter()
        .map(|p| {
            Ok(format!(
                "{}:{}",
                python_fs::json_string(p.as_os_str())?,
                serde_json::to_string(&digest(&p)?)?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let codecs = context.output(
        [
            "pkg-config",
            "--modversion",
            "libavcodec",
            "libavformat",
            "libavutil",
        ]
        .map(OsString::from)
        .into(),
        None,
    )?;
    let paths = dependencies
        .iter()
        .map(|(k, p)| {
            Ok(format!(
                "{}:{}",
                serde_json::to_string(k)?,
                python_fs::json_string(p.as_os_str())?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    dump(&options.output.join("inputs.json"),&format!("{{\"source_commit\":{},\"sources\":{{{}}},\"dependencies\":{{{}}},\"build_messages_sha256\":{},\"executables\":{{{}}},\"native_codec_versions\":{},\"native_components\":{{\"ipc\":\"openpilot-msgq\",\"cereal\":\"openpilot-cereal\",\"runtime_checker\":{}}}}}",serde_json::to_string(commit.trim())?,sources.join(","),paths.join(","),serde_json::to_string(&digest(&options.build_messages)?)?,executables.join(","),serde_json::to_string(&codecs)?,python_fs::json_string(options.binaries.join("check-encoder-runtime").as_os_str())?))
}
fn length(value: &JsonValue) -> Result<usize> {
    match value.view() {
        JsonView::Array(v) => Ok(v.len()),
        JsonView::Object(v) => Ok(v.len()),
        JsonView::Text(v) => Ok(v.len()),
        _ => Err("comparison results have no length".into()),
    }
}
/// Reserve and fingerprint before invoking any source build or comparison.
/// The public callback keeps disk refusals observable at their original phases.
pub fn execute(
    options: &Options,
    runner: &mut dyn Runner,
    space: &mut dyn FnMut(&Path, u64) -> Result<()>,
    printed: &mut dyn Write,
    errors: &mut dyn Write,
) -> Result<()> {
    let options = Options {
        root: resolve_non_strict(&options.root)?,
        binaries: resolve_non_strict(&options.binaries)?,
        build_messages: options.build_messages.clone(),
        output: resolve_non_strict(&options.output)?,
        native: resolve_non_strict(&options.native)?,
        capnp: options.capnp.clone(),
        json11_prefix: options.json11_prefix.clone(),
        json11_stager: options.json11_stager.clone(),
    };
    space(&options.output, GIB)?;
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let dependencies = dependency_outputs(&options.build_messages)?;
    identity(
        &options,
        &dependencies,
        &mut Context {
            options: &options,
            runner,
            printed,
            errors,
        },
    )?;
    let prefix = json11_dependency(&options, runner, space, printed, errors)?;
    build_source(
        &options,
        &dependencies,
        &prefix,
        runner,
        space,
        printed,
        errors,
    )?;
    for (name, program, arguments) in [
        (
            "codecs",
            "check-encoder-codecs",
            vec![
                "--rust".into(),
                os(options.binaries.join("examples/encoder_codec_trace")),
                "--output".into(),
                os(options.output.join("codecs")),
                "--jpeg-build".into(),
                os(&dependencies["jpeg"]),
                "--native".into(),
                os(&options.native),
            ],
        ),
        (
            "v4l",
            "check-encoder-v4l",
            vec![
                "--rust".into(),
                os(options.binaries.join("examples/encoder_v4l_trace")),
                "--output".into(),
                os(options.output.join("v4l")),
                "--native".into(),
                os(&options.native),
            ],
        ),
        (
            "runtime",
            "check-encoder-runtime",
            vec![
                "--rust".into(),
                os(options.binaries.join("openpilot-encoderd")),
                "--source".into(),
                os(options.output.join("source/original-encoderd")),
                "--output".into(),
                os(options.output.join("runtime")),
                "--native".into(),
                os(&options.native),
            ],
        ),
    ] {
        space(&options.output, GIB)?;
        let mut argv = vec![os(options.binaries.join(program))];
        argv.extend(arguments);
        Context {
            options: &options,
            runner,
            printed,
            errors,
        }
        .command(name, argv)?;
    }
    for (name, count) in [("codecs", 5), ("v4l", 32), ("runtime", 10)] {
        let receipt = json::parse_text(&text(fs::read(
            options.output.join(name).join("receipt.json"),
        )?)?)?;
        if get(&receipt, "status")?.to_utf8().as_deref() != Some("PASS")
            || length(&get(&receipt, "results")?)? != count
        {
            return Err(format!("incomplete encoder comparison: {name}").into());
        }
    }
    fs::write(options.output.join("receipt.json"),"{\"status\": \"PASS\", \"codec_cases\": 5, \"v4l_cases\": 32, \"runtime_cases\": 10, \"scope\": \"source/native host codecs and IPC; target drivers, device performance and complete startup/upload remain separate\"}\n")?;
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
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binaries", true),
            OptionSpec::value("--build-messages", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--native", false),
            OptionSpec::value("--capnp-prefix", false),
            OptionSpec::value("--json11-prefix", false),
            OptionSpec::value("--json11-stager", false),
        ],
    )?;
    if args.help {
        println!("usage: check-encoder-ci --binaries DIRECTORY --build-messages JSONL --output DIRECTORY [--native DIRECTORY] [--capnp-prefix DIRECTORY] [--json11-prefix DIRECTORY] [--json11-stager EXECUTABLE]\nRun the complete encoder host comparison against current Cargo artifacts");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let options = Options {
        root,
        binaries: PathBuf::from(&args.values["--binaries"]),
        build_messages: PathBuf::from(&args.values["--build-messages"]),
        output: PathBuf::from(&args.values["--output"]),
        native: args
            .values
            .get("--native")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/")),
        capnp: args
            .values
            .get("--capnp-prefix")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/usr")),
        json11_prefix: args.values.get("--json11-prefix").map(PathBuf::from),
        json11_stager: args.values.get("--json11-stager").map(PathBuf::from),
    };
    execute(
        &options,
        &mut NativeRunner,
        &mut require_space,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
}
