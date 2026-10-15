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
#[derive(Debug)]
pub struct ArgumentError(String);
impl std::fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ArgumentError {}
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
fn is_option(value: &str) -> bool {
    static NEGATIVE_NUMBER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    // Match argparse's decimal Nd digits, not the broader Unicode Number class.
    let negative = NEGATIVE_NUMBER.get_or_init(|| {
        regex::Regex::new(r"^-\d+$|^-\d*\.\d+$").expect("fixed argparse negative-number pattern")
    });
    value != "-" && value.starts_with('-') && !negative.is_match(value)
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut output = Vec::new();
        let mut root = None;
        let mut unknown = Vec::new();
        let mut at = 0;
        while at < args.len() {
            let arg = &args[at];
            if arg == "--" {
                output.extend_from_slice(&args[at + 1..]);
                break;
            }
            let (name, inline) = arg
                .split_once('=')
                .map_or((arg.as_str(), None), |(name, value)| (name, Some(value)));
            if name == "-h" || name.starts_with("--") {
                let known = ["--help", "--root"];
                let flag = if name == "-h" {
                    Some("--help")
                } else if known.contains(&name) {
                    Some(name)
                } else {
                    let matches = known
                        .iter()
                        .copied()
                        .filter(|flag| flag.starts_with(name))
                        .collect::<Vec<_>>();
                    match matches.as_slice() {
                        [flag] => Some(*flag),
                        [] => None,
                        _ => return Err(format!("ambiguous argument: {name}").into()),
                    }
                };
                match flag {
                    Some("--help") => {
                        if inline.is_some() {
                            return Err("--help does not accept a value".into());
                        }
                        return Ok(Self {
                            output: PathBuf::new(),
                            root,
                            help: true,
                        });
                    }
                    Some("--root") => {
                        let value = if let Some(value) = inline {
                            value
                        } else {
                            at += 1;
                            args.get(at)
                                .filter(|value| !is_option(value))
                                .ok_or("--root requires a value")?
                        };
                        root = Some(PathBuf::from(value));
                    }
                    _ => unknown.push(arg.clone()),
                }
            } else if name.starts_with("-h") {
                return Err("-h does not accept a value".into());
            } else if is_option(arg) {
                unknown.push(arg.clone());
            } else {
                output.push(arg.clone());
            }
            at += 1;
        }
        // argparse diagnoses deferred unknown/extra arguments only after help actions.
        if !unknown.is_empty() {
            return Err(format!("unknown arguments: {}", unknown.join(" ")).into());
        }
        if output.len() != 1 {
            return Err("exactly one output path is required".into());
        }
        Ok(Self {
            output: output_path(&output[0]),
            root,
            help: false,
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
    let options = Options::parse(&args).map_err(|e| ArgumentError(e.to_string()))?;
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
