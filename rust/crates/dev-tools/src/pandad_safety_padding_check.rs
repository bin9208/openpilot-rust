//! Original allocator-padding probe controller. Neither safety policy is ported here.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
mod ownership;
use ownership::{Bound, Publication};

pub const HELP: &str = "Usage: check-pandad-safety-padding --source SOURCE --binary BINARY --output OUTPUT\n\nCompare unchanged C++ allocator-padding controls with the native safety probe.\nNo Panda device is opened.\n";
#[derive(Debug)]
pub struct Options {
    pub root: PathBuf,
    pub source: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
}
#[derive(Debug)]
pub enum Command {
    Help,
    Run(Options),
}
pub fn parse(args: &[OsString], root: &Path) -> Result<Command> {
    let parsed = scan_os(
        args,
        &[
            OptionSpec::value("--source", true),
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        return Ok(Command::Help);
    }
    Ok(Command::Run(Options {
        root: root.into(),
        source: resolve_non_strict(Path::new(&parsed.values["--source"]))?,
        binary: resolve_non_strict(Path::new(&parsed.values["--binary"]))?,
        // The original check() creates the supplied output before resolving its
        // Params child roots. Keep that creation/error phase.
        output: PathBuf::from(&parsed.values["--output"]),
    }))
}
fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn interrupted(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "safety-padding check interrupted",
        )
        .into())
    } else {
        Ok(())
    }
}
/// Exact source-defined roster, with wire bytes supplied by the original encoder.
pub fn cases(wire: &[u8]) -> Result<Vec<Value>> {
    require(
        wire.len() >= 8,
        "original CarParams is shorter than one word",
    )?;
    Ok([false, true]
        .into_iter()
        .flat_map(|truncated| {
            [0x11, 0x22].into_iter().map(move |fill| {
                let bytes = if truncated { &wire[..wire.len() - 8] } else { wire };
                json!({"op":"safety","label":format!("{}-{fill}",if truncated {"True"} else {"False"}),"pandas":2,
                    "operations":[{"onroad":true,"params":{"CarParams":bytes,"FirmwareQueryDone":[49],"ControlsReady":[49]},"allocation_fill":fill}]})
            })
        })
        .collect())
}
fn first(row: &Value) -> Result<&Value> {
    row.get("results")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .ok_or_else(|| "missing first safety result".into())
}
fn allocated(row: &Value) -> bool {
    row.get("filled_allocations")
        .and_then(Value::as_u64)
        .is_some_and(|n| n >= 1)
}
/// Every assertion in check_pandad_safety_padding.py, including its deliberate
/// malformed-input difference. This consumes live responses, never saved answers.
pub fn compare(wire_bytes: usize, source: &[Value], native: &[Value]) -> Result<Value> {
    require(
        wire_bytes >= 8,
        "original CarParams is shorter than one word",
    )?;
    require(
        source.len() == 4 && native.len() == 4,
        "expected four source/native responses",
    )?;
    for index in 0..2 {
        let mut expected = first(&source[index])?.clone();
        require(
            allocated(&expected),
            format!("valid source control {index} did not fill allocation"),
        )?;
        expected
            .as_object_mut()
            .ok_or("safety result is not an object")?
            .remove("filled_allocations");
        require(
            expected == *first(&native[index])?,
            format!("valid safety control {index} differs"),
        )?;
    }
    require(
        first(&source[0])?["commands"] == first(&source[1])?["commands"],
        "valid safety commands depend on allocation fill",
    )?;
    let mut final_commands = Vec::new();
    for (index, fill) in [(2, 0x11_u64), (3, 0x22_u64)] {
        let actual = first(&source[index])?;
        require(
            allocated(actual)
                && actual["failed"] == false
                && actual["state"]["safety_configured"] == true,
            format!("truncated source control {index} did not expose original configured state"),
        )?;
        let final_command = actual["commands"]
            .as_array()
            .ok_or("source commands are not an array")?
            .iter()
            .rev()
            .find(|command| command["panda"] == 1 && command["request"] == 0xdc)
            .ok_or("source final safety command missing")?;
        require(
            final_command["value"] == 17 && final_command["index"] == fill * 257,
            format!("truncated source command {index} did not retain allocator padding"),
        )?;
        final_commands.push(final_command.clone());
        let rejected = first(&native[index])?;
        require(
            rejected["failed"] == true && rejected["state"]["safety_configured"] == false,
            format!("native truncated control {index} was not rejected"),
        )?;
        require(
            rejected["commands"]
                == json!([
            {"panda":0,"request":0xdc,"value":3,"index":1},
            {"panda":1,"request":0xdc,"value":3,"index":1}]),
            format!("native truncated control {index} commands differ"),
        )?;
    }
    require(
        final_commands[0] != final_commands[1],
        "original truncated final commands unexpectedly equal",
    )?;
    Ok(
        json!({"result":"pass","full_message_bytes":wire_bytes,"truncated_message_bytes":wire_bytes-8,
        "valid_input_exact_with_both_allocator_fills":true,"source_truncated_final_commands":final_commands,
        "native_rejected_both_truncated_inputs":true,
        "scope":"controlled original AlignedBuffer missing-word read; deliberate native rejection, not malformed-input parity or device evidence"}),
    )
}
fn request_bytes(rows: &[Value], params: Option<&Path>) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for row in rows {
        let text = serde_json::to_string(row)?;
        if let Some(params) = params {
            // Python json.dumps preserves surrogateescaped filesystem strings.
            // Do not narrow a raw Params path through UTF-8 or a lossy Value.
            bytes.extend_from_slice(
                text.strip_suffix('}')
                    .ok_or("request is not an object")?
                    .as_bytes(),
            );
            bytes.extend_from_slice(b",\"params_root\":");
            bytes.extend_from_slice(crate::python_fs::json_string(params.as_os_str())?.as_bytes());
            bytes.push(b'}');
        } else {
            bytes.extend_from_slice(text.as_bytes());
        }
        bytes.push(b'\n');
    }
    Ok(bytes)
}
fn invoke(
    runner: &mut dyn Runner,
    command: &Path,
    input: Vec<u8>,
    output: &Path,
    label: &str,
    count: usize,
) -> Result<Vec<Value>> {
    fs::write(output.join(format!("{label}.input.jsonl")), &input)?;
    let mut spec = CommandSpec::new_os(vec![command.into()], Duration::from_secs(90));
    spec.input = Some(input);
    let observed = runner.run(&spec)?;
    // subprocess.run(text=True) decodes both streams before writing artifacts.
    let text = |bytes: Vec<u8>| -> Result<String> {
        Ok(String::from_utf8(bytes)?
            .replace("\r\n", "\n")
            .replace('\r', "\n"))
    };
    let stdout = text(observed.stdout)?;
    let stderr = text(observed.stderr)?;
    fs::write(output.join(format!("{label}.stdout")), &stdout)?;
    fs::write(output.join(format!("{label}.stderr")), &stderr)?;
    require(
        observed.code == 0,
        format!(
            "{label} exited {}: {}",
            observed.code,
            stderr
                .chars()
                .rev()
                .take(2000)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        ),
    )?;
    let rows = stdout
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<Vec<Value>, _>>()?;
    require(
        rows.len() == count,
        format!(
            "{label} returned {} responses, expected {count}",
            rows.len()
        ),
    )?;
    Ok(rows)
}
pub struct Completed {
    pub report: Value,
    publication: Publication,
}
impl Completed {
    pub fn finish(&self, cancelled: &AtomicBool) -> Result<()> {
        if let Err(error) = interrupted(cancelled) {
            self.publication.remove_owned();
            return Err(error);
        }
        Ok(())
    }
}
/// Run the three original subprocess phases using a supervised runner. The
/// output is exclusive; earlier output trees, including receipts, are untouched.
pub fn execute(
    options: &Options,
    runner: &mut dyn Runner,
    cancel: &AtomicBool,
    stdout: &mut dyn Write,
) -> Result<Completed> {
    interrupted(cancel)?;
    let parent = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    fs::create_dir(&options.output)?;
    let pins = ownership::sources(&options.root)?;
    // Preserve original spawn failure phases/artifacts, including a missing
    // candidate after both successful source phases. Hashing is not admission
    // for an executable that never successfully runs.
    let source = Bound::open(&options.source).ok();
    let binary = Bound::open(&options.binary).ok();
    let root = resolve_non_strict(&options.output)?;
    let encoded = invoke(
        runner,
        &options.source,
        request_bytes(
            &[json!({"op":"car_params","alternative":32767,"configs":[[3,65535],[17,65435]]})],
            None,
        )?,
        &options.output,
        "car-params",
        1,
    )?;
    let wire = encoded[0]
        .as_array()
        .ok_or("original CarParams response is not a byte array")?
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| "original CarParams contains a non-byte value".into())
        })
        .collect::<Result<Vec<u8>>>()?;
    let rows = cases(&wire)?;
    interrupted(cancel)?;
    let expected = invoke(
        runner,
        &options.source,
        request_bytes(&rows, Some(&root.join("source-params")))?,
        &options.output,
        "source",
        rows.len(),
    )?;
    interrupted(cancel)?;
    let actual = invoke(
        runner,
        &options.binary,
        request_bytes(&rows, Some(&root.join("native-params")))?,
        &options.output,
        "native",
        rows.len(),
    )?;
    let report = compare(wire.len(), &expected, &actual)?;
    let source = source.ok_or("successful source probe has no regular executable identity")?;
    let binary = binary.ok_or("successful native probe has no regular executable identity")?;
    let verify = || -> Result<()> {
        interrupted(cancel)?;
        source.verify()?;
        binary.verify()?;
        for pin in &pins {
            pin.verify()?;
        }
        interrupted(cancel)
    };
    verify()?;
    let hashes: serde_json::Map<_, _> = pins
        .iter()
        .map(|pin| {
            (
                pin.relative_to(&options.root),
                Value::String(pin.hash.clone()),
            )
        })
        .collect();
    let provenance = json!({"admitted_source_sha256":hashes,"executables":{
        "source":source.hash,"native":binary.hash},"subprocess_timeout_seconds":90,
        "scope":"controller, helper, direct PandaSafety and Params admission pins; source probe build closure is recorded by build-pandad-protocol-source"});
    ownership::publish(&options.output.join("provenance.json"), &provenance)?;
    serde_json::to_writer(&mut *stdout, &report)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    verify()?;
    let publication = ownership::publish(&options.output.join("report.json"), &report)?;
    if let Err(error) = verify() {
        publication.remove_owned();
        return Err(error);
    }
    Ok(Completed {
        report,
        publication,
    })
}
struct Controlled<'a>(&'a AtomicBool);
impl Runner for Controlled<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<crate::qa_process::Captured> {
        NativeRunner.run_controlled(spec, Instant::now() + spec.timeout, self.0)
    }
}
pub fn cli() -> Result<()> {
    use std::sync::Arc;
    let root = std::env::var_os("OPENPILOT_RUST_ROOT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        PathBuf::from,
    );
    match parse(&std::env::args_os().skip(1).collect::<Vec<_>>(), &root)? {
        Command::Help => io::stdout()
            .lock()
            .write_all(HELP.as_bytes())
            .map_err(Into::into),
        Command::Run(options) => {
            let cancel = Arc::new(AtomicBool::new(false));
            let mut handlers = Vec::new();
            for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
                handlers.push(signal_hook::flag::register(signal, Arc::clone(&cancel))?);
            }
            let result = execute(
                &options,
                &mut Controlled(&cancel),
                &cancel,
                &mut io::stdout().lock(),
            )
            .and_then(|result| {
                result.finish(&cancel)?;
                Ok(result)
            });
            for handler in handlers {
                signal_hook::low_level::unregister(handler);
            }
            result.and_then(|result| result.finish(&cancel))
        }
    }
}
