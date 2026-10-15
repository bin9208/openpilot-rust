//! Build the unchanged original software encoder without Python generators.
use crate::{
    disk_space,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
    services_header,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
const GROWTH: u64 = 512 * 1024 * 1024;
const SOURCES: [&str; 17] = [
    "openpilot/system/loggerd/encoderd.cc",
    "openpilot/system/loggerd/encoder/encoder.cc",
    "openpilot/system/loggerd/encoder/ffmpeg_encoder.cc",
    "openpilot/system/loggerd/encoder/jpeg_encoder.cc",
    "openpilot/cereal/messaging/socketmaster.cc",
    "openpilot/common/params.cc",
    "openpilot/common/util.cc",
    "openpilot/common/swaglog.cc",
    "msgq_repo/msgq/ipc.cc",
    "msgq_repo/msgq/event.cc",
    "msgq_repo/msgq/impl_msgq.cc",
    "msgq_repo/msgq/impl_fake.cc",
    "msgq_repo/msgq/msgq.cc",
    "msgq_repo/msgq/visionipc/visionipc.cc",
    "msgq_repo/msgq/visionipc/visionipc_client.cc",
    "msgq_repo/msgq/visionipc/visionipc_server.cc",
    "msgq_repo/msgq/visionipc/visionbuf.cc",
];
const REQUIRED: [&str; 6] = [
    "--output",
    "--native-prefix",
    "--capnp-prefix",
    "--json11-prefix",
    "--zmq-include",
    "--jpeg-build",
];
/// Distinguish argparse-compatible input failures from runtime build failures.
#[derive(Debug)]
pub struct ArgumentError(String);
impl std::fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ArgumentError {}
struct Options {
    values: BTreeMap<String, PathBuf>,
    help: bool,
}
fn is_option(value: &str) -> bool {
    if value == "-" || !value.starts_with('-') {
        return false;
    }
    // argparse accepts negative-number path values without requiring '='.
    let negative = value.strip_prefix('-').is_some_and(|number| {
        if let Some((whole, fraction)) = number.split_once('.') {
            !fraction.is_empty()
                && whole.chars().all(char::is_numeric)
                && fraction.chars().all(char::is_numeric)
        } else {
            !number.is_empty() && number.chars().all(char::is_numeric)
        }
    });
    !negative
}
impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut values = BTreeMap::new();
        let mut at = 0;
        while at < args.len() {
            if args[at] == "--" {
                if at + 1 < args.len() {
                    return Err("unexpected positional encoder builder argument".into());
                }
                break;
            }
            let (name, inline) = args[at]
                .split_once('=')
                .map_or((args[at].as_str(), None), |(name, value)| {
                    (name, Some(value))
                });
            let known = REQUIRED
                .into_iter()
                .chain(["--root", "--help"])
                .collect::<Vec<_>>();
            let flag = if name == "-h" {
                "--help"
            } else if known.contains(&name) {
                name
            } else {
                let candidates = known
                    .iter()
                    .copied()
                    .filter(|flag| name.starts_with("--") && flag.starts_with(name))
                    .collect::<Vec<_>>();
                match candidates.as_slice() {
                    [flag] => *flag,
                    [] => return Err(format!("unknown encoder builder argument: {name}").into()),
                    _ => return Err(format!("ambiguous encoder builder argument: {name}").into()),
                }
            };
            if flag == "--help" {
                if inline.is_some() {
                    return Err("--help does not accept a value".into());
                }
                return Ok(Self { values, help: true });
            }
            let value = if let Some(value) = inline {
                value
            } else {
                at += 1;
                args.get(at)
                    .filter(|value| !is_option(value))
                    .ok_or_else(|| format!("{flag} requires a value"))?
            };
            // argparse's store action intentionally lets the last occurrence win.
            values.insert(flag.to_owned(), PathBuf::from(value));
            at += 1;
        }
        for flag in REQUIRED {
            if !values.contains_key(flag) {
                return Err(format!("{flag} is required").into());
            }
        }
        Ok(Self {
            values,
            help: false,
        })
    }
    fn path(&self, key: &str) -> &Path {
        &self.values[key]
    }
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let source = entry?.path();
        let target = target.join(source.file_name().ok_or("missing include file name")?);
        if source.is_dir() {
            copy_tree(&source, &target)?;
        } else {
            fs::copy(source, target)?;
        }
    }
    Ok(())
}
fn with_extension(path: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    Ok(fs::read_dir(path)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == extension))
        .collect())
}
/// Preserve original owned partial files and the compiler's merged raw log.
pub fn run(
    root: &Path,
    args: &[String],
    runner: &mut dyn Runner,
    free_bytes: u64,
) -> Result<PathBuf> {
    let options = Options::parse(args)?;
    if options.help {
        return Err("encoder help is handled by the CLI".into());
    }
    let output = resolve_non_strict(options.path("--output"))?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&output)?;
    write_json(
        &output.join("space.json"),
        &json!({"free":free_bytes,"growth":GROWTH,"floor":25_u64*1024*1024*1024}),
    )?;
    disk_space::Space::new(free_bytes, GROWTH)?.require()?;
    let schema = output.join("schema");
    let generated = output.join("cereal/gen/cpp");
    fs::create_dir(&schema)?;
    fs::create_dir_all(&generated)?;
    for name in ["log", "custom", "deprecated"] {
        fs::copy(
            root.join(format!("openpilot/cereal/{name}.capnp")),
            schema.join(format!("{name}.capnp")),
        )?;
    }
    fs::copy(
        root.join("opendbc_repo/opendbc/car/car.capnp"),
        schema.join("car.capnp"),
    )?;
    copy_tree(
        &root.join("openpilot/cereal/include"),
        &schema.join("include"),
    )?;
    let mut argv = vec![
        "capnp".into(),
        "compile".into(),
        format!("-I{}", schema.display()),
        format!("--src-prefix={}", schema.display()),
        format!("-oc++:{}", generated.display()),
    ];
    argv.extend(
        with_extension(&schema, "capnp")?
            .iter()
            .map(|p| p.display().to_string()),
    );
    let mut capnp = CommandSpec::new(argv, Duration::MAX);
    capnp.inherit_output = true;
    let result = runner.run(&capnp)?;
    if result.code != 0 {
        return Err(format!("encoder schema compiler exited {}", result.code).into());
    }
    fs::write(
        output.join("cereal/services.h"),
        services_header::header(true),
    )?;
    let native = resolve_non_strict(options.path("--native-prefix"))?;
    let capnp = resolve_non_strict(options.path("--capnp-prefix"))?;
    let json11 = resolve_non_strict(options.path("--json11-prefix"))?;
    let jpeg = resolve_non_strict(options.path("--jpeg-build"))?;
    let mut command: Vec<String> = ["g++", "-std=c++17", "-O1", "-g0", "-pthread"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    command.extend(
        [
            root.to_path_buf(),
            root.join("openpilot"),
            root.join("msgq_repo"),
            output.clone(),
            generated.clone(),
            json11.join("include"),
            resolve_non_strict(options.path("--zmq-include"))?,
            capnp.join("include"),
            native.join("include"),
            native.join("include/x86_64-linux-gnu"),
            root.join("rust/crates/jpeg/native/vendor/src"),
            jpeg.clone(),
        ]
        .iter()
        .map(|p| format!("-I{}", p.display())),
    );
    command.extend([
        format!("-L{}", capnp.join("lib/x86_64-linux-gnu").display()),
        format!("-L{}", native.join("lib/x86_64-linux-gnu").display()),
        format!(
            "-Wl,--disable-new-dtags,-rpath,{}",
            capnp.join("lib/x86_64-linux-gnu").display()
        ),
    ]);
    command.extend(SOURCES.iter().map(|s| root.join(s).display().to_string()));
    command.extend(
        with_extension(&generated, "c++")?
            .iter()
            .map(|p| p.display().to_string()),
    );
    command.extend([
        json11.join("lib/libjson11.a").display().to_string(),
        native
            .join("lib/x86_64-linux-gnu/libyuv.a")
            .display()
            .to_string(),
        jpeg.join("libjpeg.a").display().to_string(),
    ]);
    command.extend(
        [
            "-l:libzmq.so.5",
            "-lavformat",
            "-lavcodec",
            "-lavutil",
            "-lcapnp",
            "-lkj",
            "-o",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    let binary = output.join("original-encoderd");
    command.push(binary.display().to_string());
    write_json(&output.join("build-command.json"), &json!(command))?;
    let log = output.join("build.log");
    let mut spec = CommandSpec::new(command, Duration::from_secs(180));
    spec.merge_output = true;
    let result = runner.run_to_file(&spec, &log)?;
    if result.code != 0 {
        return Err(format!("encoder source compiler exited {}", result.code).into());
    }
    let mut sources = serde_json::Map::new();
    for source in SOURCES {
        sources.insert(source.into(), json!(digest(&root.join(source))?));
    }
    let receipt = json!({"status":"PASS","sources":sources,"binary_sha256":digest(&binary)?});
    write_json(&output.join("receipt.json"), &receipt)?;
    write_json(
        &output.join("services-provenance.json"),
        &services_header::provenance(true),
    )?;
    Ok(binary)
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let options = Options::parse(&args).map_err(|error| ArgumentError(error.to_string()))?;
    if options.help {
        println!("Usage: build-encoder-source --output OUTPUT --native-prefix PREFIX --capnp-prefix PREFIX --json11-prefix PREFIX --zmq-include INCLUDE --jpeg-build BUILD [--root ROOT]");
        return Ok(());
    }
    let root = options
        .values
        .get("--root")
        .cloned()
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))
        .canonicalize()?;
    let space = disk_space::inspect(options.path("--output"), GROWTH)?;
    let binary = run(&root, &args, &mut NativeRunner, space.free_bytes)?;
    println!("{}", binary.display());
    Ok(())
}
