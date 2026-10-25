//! Compile unchanged native logging with its original clock and ZMQ observation seams.
use crate::{
    disk_space, json11_stage,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
const SOURCES: [&str; 4] = [
    "openpilot/common/swaglog.cc",
    "openpilot/common/swaglog.h",
    "openpilot/common/timing.h",
    "rust/tools/native_logging_reference.cc",
];
fn s(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}
fn zmq_include(root: &Path) -> Result<PathBuf> {
    let build = root.join("rust/target/debug/build");
    for entry in fs::read_dir(build)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with("zmq-sys-") {
            let include = entry.path().join("out/source/include");
            if include.is_dir() {
                return Ok(include);
            }
        }
    }
    Err("build logging/msgq first or provide --zmq-include".into())
}
/// Fingerprinted source is unchanged; only nanos_since_boot returns fixture_clock.
/// The original producer continues calling real libzmq behind its two observers.
pub fn build(
    root: &Path,
    output: &Path,
    include: Option<&Path>,
    architecture: &str,
    runner: &mut dyn Runner,
    free_bytes: u64,
) -> Result<PathBuf> {
    fs::create_dir_all(output)?;
    disk_space::Space::new(free_bytes, 256 * 1024 * 1024)?.require()?;
    let staged = json11_stage::stage(root, output, architecture, runner, free_bytes)?;
    let overlay = output.join("overlay");
    fs::create_dir_all(overlay.join("common"))?;
    fs::create_dir_all(overlay.join("system/hardware"))?;
    let text = fs::read_to_string(root.join("openpilot/common/timing.h"))?;
    let start = text
        .find("static inline uint64_t nanos_since_boot()")
        .ok_or("missing nanos_since_boot clock source anchor")?;
    let end = start
        + text[start..]
            .find("\n}")
            .ok_or("missing nanos_since_boot closing source anchor")?
        + 2;
    let timing=format!("{}extern uint64_t fixture_clock;\nstatic inline uint64_t nanos_since_boot() {{ return fixture_clock; }}{}",&text[..start],&text[end..]);
    fs::write(overlay.join("common/timing.h"), timing)?;
    fs::write(
        overlay.join("common/version.h"),
        "#define COMMA_VERSION \"test-version\"\n",
    )?;
    fs::write(overlay.join("system/hardware/hw.h"),"#include <cstdlib>\n#include <cstring>\n#include <string>\nstruct Hardware { static std::string get_name() { return \"pc\"; } };\nstruct Path { static std::string swaglog_ipc() { return getenv(\"NATIVE_LOG_ENDPOINT\"); } };\n")?;
    let include = include
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(|| zmq_include(root))?;
    let binary = output.join("native_logging_reference");
    let mut command = ["c++", "-std=c++17", "-O2", "-pthread"]
        .map(str::to_owned)
        .to_vec();
    command.extend([
        format!("-I{}", overlay.display()),
        format!("-I{}", root.join("openpilot").display()),
        format!("-I{}", staged.prefix.join("include").display()),
        format!("-I{}", include.display()),
        s(root.join("rust/tools/native_logging_reference.cc")),
        s(root.join("openpilot/common/swaglog.cc")),
        s(staged.prefix.join("lib/libjson11.a")),
    ]);
    command.extend(
        [
            "-l:libzmq.so.5",
            "-Wl,--wrap=zmq_send",
            "-Wl,--wrap=zmq_setsockopt",
            "-o",
        ]
        .map(str::to_owned),
    );
    command.push(s(&binary));
    let result = runner.run(&CommandSpec::new(command.clone(), Duration::from_secs(300)))?;
    if result.code != 0 {
        return Err(format!(
            "native logging source compiler exited {}: {}",
            result.code,
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    let mut log = result.stdout;
    log.extend(result.stderr);
    fs::write(output.join("build.log"), log)?;
    let mut hashes = serde_json::Map::new();
    for source in SOURCES {
        hashes.insert(source.into(), json!(digest(&root.join(source))?));
    }
    let mut provenance = staged
        .dependency
        .as_object()
        .ok_or("json11 dependency metadata missing")?
        .clone();
    provenance.insert("command".into(), json!(command));
    provenance.insert("sources".into(), json!(hashes));
    provenance.insert("clock_override".into(),json!("Only nanos_since_boot returns fixture_clock for exact rate-limit boundaries; realtime source unchanged."));
    provenance.insert("wrappers".into(),json!("zmq_send and zmq_setsockopt observe arguments/results and call actual libzmq unchanged."));
    fs::write(
        output.join("provenance.json"),
        format!("{}\n", serde_json::to_string_pretty(&provenance)?),
    )?;
    Ok(binary)
}
pub fn cli() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut values = BTreeMap::new();
    while let Some(key) = args.next() {
        if key == "--help" || key == "-h" {
            println!("Usage: build-native-logging-reference --root ROOT --output OUTPUT [--zmq-include INCLUDE] [--architecture x86_64|aarch64]");
            return Ok(());
        }
        if !["--root", "--output", "--zmq-include", "--architecture"].contains(&key.as_str()) {
            return Err("unknown native logging builder argument".into());
        }
        let value = args.next().ok_or("argument requires value")?;
        if values.insert(key, value).is_some() {
            return Err("duplicate native logging builder argument".into());
        }
    }
    let root = PathBuf::from(values.remove("--root").ok_or("--root required")?).canonicalize()?;
    let output = crate::path_resolve::resolve_non_strict(&PathBuf::from(
        values.remove("--output").ok_or("--output required")?,
    ))?;
    let include = values.remove("--zmq-include").map(PathBuf::from);
    let arch = values
        .remove("--architecture")
        .unwrap_or_else(|| std::env::consts::ARCH.into());
    let space = disk_space::inspect(&output, 256 * 1024 * 1024)?;
    let binary = build(
        &root,
        &output,
        include.as_deref(),
        &arch,
        &mut NativeRunner,
        space.free_bytes,
    )?;
    println!("{}", binary.display());
    Ok(())
}
