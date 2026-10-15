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
pub use crate::argparse_scan::ArgumentError;
fn parse_arguments(
    args: &[String],
) -> std::result::Result<crate::argparse_scan::Parsed, ArgumentError> {
    use crate::argparse_scan::{scan, OptionSpec as S};
    scan(
        args,
        &[
            S::value("--output", true),
            S::value("--target", false),
            S::value("--zmq-root", false),
            S::value("--root", false),
        ],
    )
}
struct Options {
    output: PathBuf,
    target: PathBuf,
    zmq: Option<PathBuf>,
}
impl Options {
    fn parse(root: &Path, args: &[String]) -> Result<Self> {
        let mut values = parse_arguments(args)?.values;
        Ok(Self {
            output: PathBuf::from(values.remove("--output").ok_or("--output required")?),
            target: values
                .remove("--target")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("rust/target")),
            zmq: values.remove("--zmq-root").map(PathBuf::from),
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
            candidates.into_iter().find(|p|p.join("lib/libzmq.a").is_file()&&p.join("source/include/zmq.h").is_file()).ok_or_else(|| ArgumentError("build openpilot-bridge first or provide --zmq-root containing lib/libzmq.a and source/include/zmq.h".into()))?
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
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let parsed = parse_arguments(&args)?;
    if parsed.help {
        println!("Usage: build-bridge-source --output OUTPUT [--zmq-root ROOT] [--target TARGET] [--root ROOT]");
        return Ok(());
    }
    let root = parsed
        .values
        .get("--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let root = root.canonicalize()?;
    let options = Options::parse(&root, &args)?;
    let space = disk_space::inspect(&options.output, GROWTH)?;
    run(&root, &args, &mut NativeRunner, space.free_bytes)?;
    Ok(())
}

#[cfg(test)]
mod original_cli_interfaces {
    #[test]
    fn original_argparse_admission() {
        let rows: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/panda-cli/five-interfaces.json"
        ))
        .unwrap();
        for row in rows
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| ["bridge"].contains(&row["family"].as_str().unwrap()))
        {
            let args = row["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            let result = super::parse_arguments(&args);
            assert_eq!(result.is_ok(), row["original_exit"] == 0, "{row}");
            if let Ok(parsed) = result {
                assert_eq!(parsed.help, row["help"].as_bool().unwrap(), "{row}");
                if !parsed.help {
                    for (key, value) in &parsed.values {
                        let name = key.trim_start_matches("--").replace('-', "_");
                        let expected = &row["parsed"][&name];
                        if expected.is_boolean() {
                            assert_eq!(expected, true, "{row}");
                        } else {
                            assert_eq!(value, expected.as_str().unwrap(), "{row}");
                        }
                    }
                    if row["family"] == "bootlog" {
                        assert_eq!(
                            parsed.positionals[0],
                            row["parsed"]["output"].as_str().unwrap(),
                            "{row}"
                        );
                    }
                }
            }
        }
    }
}
