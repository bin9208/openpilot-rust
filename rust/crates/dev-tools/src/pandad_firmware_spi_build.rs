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
pub use crate::argparse_scan::ArgumentError;
struct Options {
    values: BTreeMap<String, String>,
    help: bool,
}
impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        use crate::argparse_scan::{scan, OptionSpec};
        let mut specs = REQUIRED.map(|name| OptionSpec::value(name, true)).to_vec();
        specs.push(OptionSpec::value("--root", false));
        let parsed = scan(args, &specs)?;
        Ok(Self {
            values: parsed.values,
            help: parsed.help,
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
    let options = Options::parse(&args)?;
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
            .filter(|row| row["family"] == "firmware")
        {
            let mut args = row["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            args.extend(
                ["--python-include", "include", "--extension-suffix", ".so"].map(str::to_owned),
            );
            let parsed = super::Options::parse(&args);
            assert_eq!(parsed.is_ok(), row["exit"] == 0, "{}", row);
            if let Ok(parsed) = parsed {
                assert_eq!(parsed.help, row["help"].as_bool().unwrap(), "{}", row);
                if !parsed.help {
                    for (name, value) in row["parsed"].as_object().unwrap() {
                        let key = format!("--{}", name.replace('_', "-"));
                        assert_eq!(parsed.values[&key], value.as_str().unwrap(), "{row}");
                    }
                }
            }
        }
    }
}
