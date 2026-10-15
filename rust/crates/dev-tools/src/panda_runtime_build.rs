//! Compile unchanged original pandad with a Rust-generated service header.
use crate::{
    disk_space,
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
const SOURCES: [&str; 16] = [
    "openpilot/selfdrive/pandad/main.cc",
    "openpilot/selfdrive/pandad/pandad.cc",
    "openpilot/selfdrive/pandad/panda.cc",
    "openpilot/selfdrive/pandad/panda_safety.cc",
    "openpilot/selfdrive/pandad/panda_comms.cc",
    "openpilot/selfdrive/pandad/spi.cc",
    "openpilot/common/params.cc",
    "openpilot/common/util.cc",
    "openpilot/common/ratekeeper.cc",
    "openpilot/common/swaglog.cc",
    "openpilot/cereal/messaging/socketmaster.cc",
    "msgq_repo/msgq/ipc.cc",
    "msgq_repo/msgq/event.cc",
    "msgq_repo/msgq/impl_msgq.cc",
    "msgq_repo/msgq/impl_fake.cc",
    "msgq_repo/msgq/msgq.cc",
];
const HEADERS: [&str; 11] = [
    "openpilot/selfdrive/pandad/panda.h",
    "openpilot/selfdrive/pandad/pandad.h",
    "openpilot/selfdrive/pandad/panda_comms.h",
    "openpilot/selfdrive/pandad/spi_alert.h",
    "openpilot/common/params.h",
    "openpilot/common/params_keys.h",
    "openpilot/common/ratekeeper.h",
    "openpilot/common/util.h",
    "openpilot/common/swaglog.h",
    "openpilot/cereal/messaging/messaging.h",
    "openpilot/system/hardware/pc/hardware.h",
];
pub use crate::argparse_scan::ArgumentError;
fn parse_arguments(
    args: &[String],
) -> std::result::Result<crate::argparse_scan::Parsed, ArgumentError> {
    use crate::argparse_scan::{scan, OptionSpec as S};
    let mut specs = [
        "--output",
        "--generated-root",
        "--capnp-prefix",
        "--json11-prefix",
        "--libusb-include",
        "--zmq-root",
        "--fixture",
    ]
    .map(|name| S::value(name, true))
    .to_vec();
    specs.extend([S::flag("--sanitize", None), S::value("--root", false)]);
    scan(args, &specs)
}
struct Options {
    values: BTreeMap<String, PathBuf>,
    sanitize: bool,
}
impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let parsed = parse_arguments(args)?;
        if parsed.help {
            return Err(ArgumentError("help is handled by the CLI".into()).into());
        }
        let sanitize = parsed.values.contains_key("--sanitize");
        let mut values = parsed
            .values
            .into_iter()
            .filter(|(key, _)| key != "--sanitize")
            .map(|(key, value)| (key, PathBuf::from(value)))
            .collect::<BTreeMap<_, _>>();
        values.insert(
            "--output".into(),
            crate::path_resolve::resolve_non_strict(&values["--output"])?,
        );
        Ok(Self { values, sanitize })
    }
    fn path(&self, key: &str) -> &Path {
        &self.values[key]
    }
}
fn s(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
/// Keep partial outputs and compiler logs after failure, as the original builder did.
pub fn run(root: &Path, args: &[String], runner: &mut dyn Runner, free_bytes: u64) -> Result<()> {
    let o = Options::parse(args)?;
    let output = o.path("--output");
    fs::create_dir_all(output.parent().ok_or("output parent missing")?)?;
    fs::create_dir(output)?;
    disk_space::Space::new(free_bytes, 512 * 1024 * 1024)?.require()?;
    fs::create_dir(output.join("cereal"))?;
    fs::write(
        output.join("cereal/services.h"),
        services_header::header(false),
    )?;
    let sources = SOURCES.map(|path| root.join(path));
    let generated = o.path("--generated-root").join("cereal/gen/cpp");
    let mut generated_sources = fs::read_dir(&generated)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    generated_sources.retain(|path| path.extension().is_some_and(|extension| extension == "c++"));
    generated_sources.sort();
    if generated_sources.len() != 4 {
        return Err("exactly four generated C++ schema units required".into());
    }
    let binary = output.join("pandad-source");
    let library = o.path("--capnp-prefix").join("lib/x86_64-linux-gnu");
    let fixture = o.path("--fixture");
    let fixture_parent = fixture.parent().ok_or("fixture parent missing")?;
    let mut command = [
        "clang++",
        "-std=c++17",
        "-O1",
        "-g",
        "-ffunction-sections",
        "-fdata-sections",
        "-Wl,--gc-sections",
    ]
    .map(str::to_owned)
    .to_vec();
    for include in [
        root.to_path_buf(),
        root.join("openpilot"),
        root.join("msgq_repo"),
        output.to_path_buf(),
        o.path("--generated-root").to_path_buf(),
        generated,
        o.path("--capnp-prefix").join("include"),
        o.path("--json11-prefix").join("include"),
        o.path("--libusb-include").to_path_buf(),
        o.path("--zmq-root").join("source/include"),
    ] {
        command.push(format!("-I{}", include.display()));
    }
    command.extend(sources.iter().chain(generated_sources.iter()).map(s));
    command.extend([
        s(o.path("--json11-prefix").join("lib/libjson11.a")),
        s(o.path("--zmq-root").join("lib/libzmq.a")),
        format!("-L{}", fixture_parent.display()),
        format!("-Wl,-rpath,{}", fixture_parent.display()),
        "-l:libusb-1.0.so.0".into(),
        format!("-L{}", library.display()),
        format!("-Wl,-rpath,{}", library.display()),
    ]);
    command.extend(["-lcapnp", "-lkj", "-pthread", "-ldl", "-o"].map(str::to_owned));
    command.push(s(&binary));
    if o.sanitize {
        command.splice(
            1..1,
            ["-fsanitize=address,undefined", "-fno-omit-frame-pointer"].map(str::to_owned),
        );
    }
    let captured = runner.run(&CommandSpec::new(command.clone(), Duration::from_secs(600)))?;
    write_json(
        &output.join("command.json"),
        &json!({"argv":command,"exit_code":captured.code,"stdout":String::from_utf8_lossy(&captured.stdout),"stderr":String::from_utf8_lossy(&captured.stderr)}),
    )?;
    if captured.code != 0 {
        return Err(format!("pandad runtime source compiler exited {}", captured.code).into());
    }
    let mut files = sources.to_vec();
    files.extend(generated_sources);
    files.extend(HEADERS.map(|path| root.join(path)));
    // Original catalog provenance remains explicit until the shared source retires.
    // This file is fingerprinted as data; the native builder never executes it.
    files.push(root.join("openpilot/cereal/services.py"));
    let mut hashes = serde_json::Map::new();
    for path in files {
        hashes.insert(s(&path), json!(digest(&path)?));
    }
    let report = json!({"native_generator":services_header::provenance(false),"result":"PASS","binary":s(&binary),"sha256":digest(&binary)?,"source_sha256":hashes,"fixture_sha256":digest(fixture)?,"sanitize":o.sanitize,"scope":"unchanged C++ pandad/main/state/safety/peripheral/serial/CAN loops and msgq, fixture USB, host hardware path"});
    write_json(&output.join("manifest.json"), &report)?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let parsed = parse_arguments(&args)?;
    if parsed.help {
        println!("Usage: build-pandad-runtime-source --output OUTPUT --generated-root ROOT --capnp-prefix PREFIX --json11-prefix PREFIX --libusb-include INCLUDE --zmq-root ROOT --fixture LIBRARY [--sanitize] [--root ROOT]");
        return Ok(());
    }
    let root = parsed
        .values
        .get("--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let root = root.canonicalize()?;
    let options = Options::parse(&args)?;
    let space = disk_space::inspect(options.path("--output"), 512 * 1024 * 1024)?;
    run(&root, &args, &mut NativeRunner, space.free_bytes)
}

#[cfg(test)]
mod original_cli_interfaces {
    #[test]
    fn original_argparse_admission() {
        assert!(super::Options::parse(&["--help".into()]).is_err());
        let rows: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/panda-cli/five-interfaces.json"
        ))
        .unwrap();
        for row in rows
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| ["runtime"].contains(&row["family"].as_str().unwrap()))
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
