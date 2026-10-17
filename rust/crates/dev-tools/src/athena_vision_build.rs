//! Build the unchanged original Athena VisionIPC peer without Python.
use crate::{
    disk_space,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
const GROWTH: u64 = 1024 * 1024 * 1024;
const SOURCES: [&str; 9] = [
    "ipc.cc",
    "event.cc",
    "impl_msgq.cc",
    "impl_fake.cc",
    "msgq.cc",
    "visionipc/visionipc.cc",
    "visionipc/visionipc_client.cc",
    "visionipc/visionipc_server.cc",
    "visionipc/visionbuf.cc",
];
pub use crate::argparse_scan::ArgumentError;
struct Options {
    output: PathBuf,
    root: Option<PathBuf>,
    help: bool,
}
// pathlib.Path removes redundant separators and '.' but does not resolve '..'
// or symlinks. Exactly two leading slashes remain a distinct POSIX anchor.
fn output_path(value: &str) -> PathBuf {
    let prefix = if value.starts_with("//") && !value.starts_with("///") {
        "//"
    } else if value.starts_with('/') {
        "/"
    } else {
        ""
    };
    let tail = value
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/");
    let path = format!("{prefix}{tail}");
    PathBuf::from(if path.is_empty() { ".".into() } else { path })
}
impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        use crate::argparse_scan::{scan_with_positionals, OptionSpec};
        let parsed = scan_with_positionals(args, &[OptionSpec::value("--root", false)], 1)?;
        Ok(Self {
            output: parsed
                .positionals
                .first()
                .map(|value| output_path(value))
                .unwrap_or_default(),
            root: parsed.values.get("--root").map(PathBuf::from),
            help: parsed.help,
        })
    }
}
/// CXX remains one executable name; compiler flags and output strings are unchanged.
pub fn run(
    root: &Path,
    args: &[String],
    compiler: &str,
    runner: &mut dyn Runner,
    free_bytes: u64,
) -> Result<PathBuf> {
    let options = Options::parse(args)?;
    if options.help {
        return Err("Athena vision help is handled by the CLI".into());
    }
    disk_space::Space::new(free_bytes, GROWTH)?.require()?;
    let parent = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut argv = vec![
        compiler.into(),
        "-std=c++17".into(),
        "-O1".into(),
        "-pthread".into(),
        "-UNDEBUG".into(),
        "-I".into(),
        root.join("msgq_repo").display().to_string(),
    ];
    argv.extend(
        SOURCES
            .iter()
            .map(|s| root.join("msgq_repo/msgq").join(s).display().to_string()),
    );
    argv.extend([
        root.join("rust/crates/athena/tests/native/vision_peer.cc")
            .display()
            .to_string(),
        "-o".into(),
        options.output.display().to_string(),
    ]);
    let mut spec = CommandSpec::new(argv, Duration::MAX);
    spec.inherit_output = true;
    let result = runner.run(&spec)?;
    if result.code != 0 {
        return Err(format!("original VisionIPC peer compiler exited {}", result.code).into());
    }
    Ok(options.output)
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let options = Options::parse(&args)?;
    if options.help {
        println!("Usage: build-athena-vision-peer [-h|--help] [--root ROOT] OUTPUT");
        return Ok(());
    }
    let root = options
        .root
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))
        .canonicalize()?;
    let free = disk_space::inspect(&root, GROWTH)?.free_bytes;
    let compiler = std::env::var("CXX").unwrap_or_else(|_| "c++".into());
    let output = run(&root, &args, &compiler, &mut NativeRunner, free)?;
    println!(
        "PASS: owned original-VisionIPC peer compiled {}",
        output.display()
    );
    Ok(())
}

#[cfg(test)]
mod original_cli_interfaces {
    #[test]
    fn original_argparse_admission() {
        let rows: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/panda-cli/four-interfaces.json"
        ))
        .unwrap();
        for row in rows
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["family"] == "athena")
        {
            let args = row["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            let parsed = super::Options::parse(&args);
            assert_eq!(parsed.is_ok(), row["exit"] == 0, "{}", row);
            if let Ok(parsed) = parsed {
                assert_eq!(parsed.help, row["help"].as_bool().unwrap(), "{}", row);
                if !parsed.help {
                    assert_eq!(
                        parsed.output.to_string_lossy(),
                        row["parsed"]["output"].as_str().unwrap(),
                        "{row}"
                    );
                }
            }
        }
    }
}
