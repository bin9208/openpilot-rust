//! Build the original syscall fixture and explicit-ABI spidev reference binding.
use crate::{
    disk_space,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
const GROWTH: u64 = 512 * 1024 * 1024;
const REQUIRED: [&str; 5] = [
    "--output",
    "--json11-prefix",
    "--spidev-source",
    "--python-include",
    "--extension-suffix",
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
    values: BTreeMap<String, String>,
    help: bool,
}
fn is_option(value: &str) -> bool {
    static NEGATIVE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let negative = NEGATIVE.get_or_init(|| {
        regex::Regex::new(r"^-\d+$|^-\d*\.\d+$").expect("fixed argparse negative-number pattern")
    });
    value != "-" && value.starts_with('-') && !negative.is_match(value)
}
impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut values = BTreeMap::new();
        let mut unknown = Vec::new();
        let mut at = 0;
        while at < args.len() {
            let arg = &args[at];
            if arg == "--" {
                return Err("unexpected positional firmware SPI argument".into());
            }
            let (name, inline) = arg
                .split_once('=')
                .map_or((arg.as_str(), None), |(name, value)| (name, Some(value)));
            let known = REQUIRED
                .into_iter()
                .chain(["--root", "--help"])
                .collect::<Vec<_>>();
            let flag = if name == "-h" {
                Some("--help")
            } else if known.contains(&name) {
                Some(name)
            } else if name.starts_with("--") {
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
            } else if name.starts_with("-h") {
                return Err("-h does not accept a value".into());
            } else {
                None
            };
            if let Some(flag) = flag {
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
                values.insert(flag.to_owned(), value.to_owned());
            } else {
                unknown.push(arg.clone());
            }
            at += 1;
        }
        if !unknown.is_empty() {
            return Err(format!("unknown firmware SPI arguments: {}", unknown.join(" ")).into());
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
    fn value(&self, key: &str) -> &str {
        &self.values[key]
    }
    fn path(&self, key: &str) -> PathBuf {
        lexical(self.value(key))
    }
}
// Match pathlib's lexical normalization, retaining '..' and symlink traversal.
fn lexical(value: &str) -> PathBuf {
    let prefix = if value.starts_with("//") && !value.starts_with("///") {
        "//"
    } else if value.starts_with('/') {
        "/"
    } else {
        ""
    };
    let tail = value
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect::<Vec<_>>()
        .join("/");
    let path = format!("{prefix}{tail}");
    PathBuf::from(if path.is_empty() { ".".into() } else { path })
}
fn join(parent: &Path, child: &str) -> PathBuf {
    lexical(&parent.join(child).to_string_lossy())
}
fn s(path: &Path) -> String {
    path.display().to_string()
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
/// Only the build driver is native; the reference C extension remains unchanged.
pub fn run(
    root: &Path,
    args: &[String],
    runner: &mut dyn Runner,
    free_bytes: u64,
) -> Result<Value> {
    let options = Options::parse(args)?;
    if options.help {
        return Err("firmware SPI help is handled by the CLI".into());
    }
    disk_space::Space::new(free_bytes, GROWTH)?.require()?;
    let output = options.path("--output");
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    fs::create_dir(&output)?;
    let fixture = join(&output, "libpandad_firmware_spi.so");
    let binding = join(
        &output,
        &format!("spidev{}", options.value("--extension-suffix")),
    );
    let source = root.join("rust/tools/pandad_firmware_spidev.cc");
    let spidev = options.path("--spidev-source");
    let json11 = options.path("--json11-prefix");
    let module = join(&spidev, "spidev_module.c");
    let mut first: Vec<String> = [
        "clang++",
        "-std=c++17",
        "-O1",
        "-g",
        "-shared",
        "-fPIC",
        "-Wall",
        "-Wextra",
        "-Werror",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    first.extend([
        format!("-I{}", join(&json11, "include").display()),
        s(&source),
        s(&join(&json11, "lib/libjson11.a")),
        "-ldl".into(),
        "-o".into(),
        s(&fixture),
    ]);
    let second = vec![
        "cc".into(),
        "-O1".into(),
        "-g".into(),
        "-shared".into(),
        "-fPIC".into(),
        format!("-I{}", options.value("--python-include")),
        s(&module),
        "-o".into(),
        s(&binding),
    ];
    let mut logs = Vec::new();
    for command in [first, second] {
        let mut spec = CommandSpec::new(command.clone(), Duration::MAX);
        // Python capture_output retains all diagnostics in memory, separately per stream.
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        let captured = runner.run(&spec)?;
        let code = captured.code;
        let stdout = text(captured.stdout)?;
        let stderr = text(captured.stderr)?;
        logs.push(json!({"argv":command,"exit_code":code,"stdout":stdout,"stderr":stderr}));
        write_json(&join(&output, "commands.json"), &json!(logs))?;
        if code != 0 {
            return Err(format!("firmware SPI compiler exited {code}").into());
        }
    }
    let fixture_resolved = resolve_non_strict(&fixture)?;
    let binding_resolved = resolve_non_strict(&binding)?;
    let mut hashes = serde_json::Map::new();
    for path in [
        &source,
        &module,
        &join(&spidev, "LICENSE"),
        &fixture,
        &binding,
    ] {
        hashes.insert(s(path), json!(digest(path)?));
    }
    let report = json!({"result":"PASS","fixture":fixture_resolved,"binding":binding_resolved,"sha256":hashes,"limits":"Owned spidev syscall fixture backed by /dev/null; original spidev 3.8 binding; no physical SPI."});
    write_json(&join(&output, "manifest.json"), &report)?;
    Ok(report)
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let options = Options::parse(&args).map_err(|e| ArgumentError(e.to_string()))?;
    if options.help {
        println!("Usage: build-pandad-firmware-spi-fixture --output OUTPUT --json11-prefix PREFIX --spidev-source SOURCE --python-include INCLUDE --extension-suffix SUFFIX [--root ROOT]");
        return Ok(());
    }
    let root = options
        .values
        .get("--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))
        .canonicalize()?;
    let free = disk_space::inspect(&options.path("--output"), GROWTH)?.free_bytes;
    let report = run(&root, &args, &mut NativeRunner, free)?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
