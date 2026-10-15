//! Build the unchanged native messaging bridge with the Rust service catalog.
use crate::{
    disk_space,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
    services_header,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
const GROWTH: u64 = 512 * 1024 * 1024;
const SOURCES: [&str; 8] = [
    "openpilot/cereal/messaging/bridge.cc",
    "openpilot/cereal/messaging/bridge_zmq.cc",
    "openpilot/cereal/messaging/msgq_to_zmq.cc",
    "msgq_repo/msgq/ipc.cc",
    "msgq_repo/msgq/event.cc",
    "msgq_repo/msgq/impl_msgq.cc",
    "msgq_repo/msgq/impl_fake.cc",
    "msgq_repo/msgq/msgq.cc",
];
struct Options {
    output: PathBuf,
    target: PathBuf,
    zmq: Option<PathBuf>,
}
impl Options {
    fn parse(root: &Path, args: &[String]) -> Result<Self> {
        let mut output = None;
        let mut target = None;
        let mut zmq = None;
        let mut at = 0;
        while at < args.len() {
            let slot = match args[at].as_str() {
                "--output" => &mut output,
                "--target" => &mut target,
                "--zmq-root" => &mut zmq,
                _ => return Err(format!("unknown bridge builder argument: {}", args[at]).into()),
            };
            if slot.is_some() {
                return Err(format!("duplicate {}", args[at]).into());
            }
            let value = args
                .get(at + 1)
                .filter(|v| !v.starts_with("--"))
                .ok_or_else(|| format!("{} requires a value", args[at]))?;
            *slot = Some(PathBuf::from(value));
            at += 2;
        }
        Ok(Self {
            output: output.ok_or("--output is required")?,
            target: target.unwrap_or_else(|| root.join("rust/target")),
            zmq,
        })
    }
    fn zmq_root(&self) -> Result<PathBuf> {
        let root = if let Some(root) = &self.zmq {
            root.clone()
        } else {
            let mut candidates = fs::read_dir(self.target.join("debug/build"))
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("zmq-sys-"))
                .map(|entry| entry.path().join("out"))
                .collect::<Vec<_>>();
            candidates.sort();
            candidates.into_iter().find(|p|p.join("lib/libzmq.a").is_file()&&p.join("source/include/zmq.h").is_file()).ok_or("build openpilot-bridge first or provide --zmq-root containing lib/libzmq.a and source/include/zmq.h")?
        };
        resolve_non_strict(&root)
    }
}
/// Compiler streams remain inherited and partial outputs remain owned by the caller.
pub fn run(
    root: &Path,
    args: &[String],
    runner: &mut dyn Runner,
    free_bytes: u64,
) -> Result<PathBuf> {
    let options = Options::parse(root, args)?;
    let zmq = options.zmq_root()?;
    let output = resolve_non_strict(&options.output)?;
    disk_space::Space::new(free_bytes, GROWTH)?.require()?;
    let include = output.join("include/cereal");
    fs::create_dir_all(&include)?;
    fs::write(include.join("services.h"), services_header::header(true))?;
    let mut argv: Vec<String> = ["g++", "-std=c++17", "-pthread", "-O2"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    for path in [
        output.join("include"),
        root.join("openpilot"),
        root.join("msgq_repo"),
        zmq.join("source/include"),
    ] {
        argv.push("-I".into());
        argv.push(path.display().to_string());
    }
    argv.extend(
        SOURCES
            .iter()
            .map(|source| root.join(source).display().to_string()),
    );
    let binary = output.join("bridge-original");
    argv.extend([
        zmq.join("lib/libzmq.a").display().to_string(),
        "-o".into(),
        binary.display().to_string(),
    ]);
    let mut command = CommandSpec::new(argv, Duration::from_secs(120));
    command.inherit_output = true;
    let captured = runner.run(&command)?;
    if captured.code != 0 {
        return Err(format!("bridge source compiler exited {}", captured.code).into());
    }
    Ok(binary)
}
pub fn cli() -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: build-bridge-source --output OUTPUT [--zmq-root ROOT] [--target TARGET] [--root ROOT]");
        return Ok(());
    }
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    if let Some(at) = args.iter().position(|arg| arg == "--root") {
        root = PathBuf::from(args.get(at + 1).ok_or("--root requires a value")?);
        args.drain(at..at + 2);
    }
    let root = root.canonicalize()?;
    let options = Options::parse(&root, &args)?;
    let space = disk_space::inspect(&options.output, GROWTH)?;
    run(&root, &args, &mut NativeRunner, space.free_bytes)?;
    Ok(())
}
