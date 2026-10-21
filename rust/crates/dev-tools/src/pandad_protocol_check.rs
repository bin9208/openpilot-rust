//! Original Panda CAN codec/SPI alert QA controller; fixture transports only.
use crate::{
    argparse_scan::{scan_os, ArgumentError, OptionSpec},
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
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
mod cases;
mod ownership;
pub use cases::{alerts, decoding, packing, seeds, INVALID};
use ownership::{Bound, Publication};
/// Complete report including Python filesystem strings that serde_json::Value
/// cannot represent. RawValue preserves their escaped codepoints on publication.
#[derive(serde::Serialize)]
pub struct Report {
    #[serde(flatten)]
    fields: Value,
    native_command: Box<serde_json::value::RawValue>,
}
impl Report {
    pub fn json_value(&self) -> Result<openpilot_log_format::JsonValue> {
        Ok(openpilot_log_format::JsonValue::parse(
            &serde_json::to_string(self)?,
        )?)
    }
}
pub fn command_json(command: &[OsString]) -> Result<Box<serde_json::value::RawValue>> {
    let values = command
        .iter()
        .map(|arg| crate::python_fs::json_string(arg))
        .collect::<Result<Vec<_>>>()?;
    Ok(serde_json::value::RawValue::from_string(format!(
        "[{}]",
        values.join(",")
    ))?)
}
pub const HELP: &str="Usage: check-pandad-protocol --source SOURCE --binary BINARY --output OUTPUT [--qemu QEMU --sysroot SYSROOT]\n";
pub const SCOPE: &str =
    "unchanged C++ CAN codec and SPI alert tracker; no physical transport or device validation";
#[derive(Clone, Debug)]
pub struct Options {
    pub root: PathBuf,
    pub source: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
    pub qemu: Option<PathBuf>,
    pub sysroot: Option<PathBuf>,
}
pub fn parse(args: &[OsString], root: &Path) -> Result<Option<Options>> {
    let parsed = scan_os(
        args,
        &[
            OptionSpec::value("--source", true),
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--qemu", false),
            OptionSpec::value("--sysroot", false),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    let qemu = parsed.values.get("--qemu").map(PathBuf::from);
    let sysroot = parsed.values.get("--sysroot").map(PathBuf::from);
    if qemu.is_some() != sysroot.is_some() {
        return Err(ArgumentError("--qemu and --sysroot must be supplied together".into()).into());
    }
    Ok(Some(Options {
        root: root.into(),
        source: PathBuf::from(&parsed.values["--source"]),
        binary: PathBuf::from(&parsed.values["--binary"]),
        output: PathBuf::from(&parsed.values["--output"]),
        qemu,
        sysroot,
    }))
}
fn cancel(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::Relaxed) {
        return Err(io::Error::from(io::ErrorKind::Interrupted).into());
    }
    Ok(())
}
fn integer(value: &mut Value) -> Result<()> {
    let number = if let Some(text) = value.as_str() {
        text.parse::<u64>()?
    } else {
        value
            .as_u64()
            .ok_or("protocol alert time is not an unsigned integer")?
    };
    *value = json!(number);
    Ok(())
}
pub fn normalize(requests: &[Value], values: &mut [Value]) -> Result<()> {
    if requests.len() != values.len() {
        return Err("Panda protocol incomplete response trace".into());
    }
    for (request, value) in requests.iter().zip(values) {
        if request["op"] == "alerts" {
            for state in value["results"]
                .as_array_mut()
                .ok_or("protocol alert results not an array")?
            {
                integer(state.get_mut("since").ok_or("protocol since missing")?)?;
                integer(
                    state
                        .get_mut("pending_since")
                        .ok_or("protocol pending_since missing")?,
                )?;
                for time in state["times"]
                    .as_array_mut()
                    .ok_or("protocol event times not an array")?
                {
                    integer(time)?;
                }
            }
        }
    }
    Ok(())
}
pub fn validate_flags(expected: &[Value], pack_count: usize, valid_count: usize) -> Result<()> {
    for value in expected
        .get(pack_count..pack_count + valid_count)
        .ok_or("protocol flag fixture range missing")?
    {
        let results = value["results"]
            .as_array()
            .ok_or("protocol flag results missing")?;
        if results
            .iter()
            .any(|row| row["ok"] != true || row["resets"] != 0)
            || results
                .last()
                .and_then(|row| row["frames"].as_array())
                .is_none_or(|frames| frames.len() != 1)
        {
            return Err("invalid protocol receive flag fixture".into());
        }
    }
    Ok(())
}
fn payload(rows: &[Value]) -> String {
    rows.iter().map(|row| format!("{row}\n")).collect()
}
fn run(
    runner: &mut dyn Runner,
    argv: &[OsString],
    rows: &[Value],
    output: &Path,
    label: &str,
    flag: &AtomicBool,
) -> Result<Vec<Value>> {
    cancel(flag)?;
    let input = payload(rows);
    fs::write(output.join(format!("{label}.input.jsonl")), &input)?;
    let mut spec = CommandSpec::new_os(argv.to_vec(), Duration::from_secs(45));
    spec.input = Some(input.into_bytes());
    spec.disable_core_dump = true;
    // Large decode transcripts include accumulated frames; bounded independently
    // from the short controller stdout/stderr contract.
    spec.stdout_limit = 128 * 1024 * 1024;
    spec.stderr_limit = 8 * 1024 * 1024;
    let child = runner.run(&spec)?;
    let text = |bytes| -> Result<String> {
        Ok(String::from_utf8(bytes)?
            .replace("\r\n", "\n")
            .replace('\r', "\n"))
    };
    let stdout = text(child.stdout)?;
    let stderr = text(child.stderr)?;
    fs::write(output.join(format!("{label}.stdout")), &stdout)?;
    fs::write(output.join(format!("{label}.stderr")), &stderr)?;
    if child.code != 0 {
        return Err(format!("Panda protocol {label} exited {}", child.code).into());
    }
    let mut values = stdout
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<Vec<Value>, _>>()?;
    normalize(rows, &mut values)?;
    cancel(flag)?;
    Ok(values)
}
fn same(expected: &[Value], actual: &[Value]) -> Result<()> {
    if expected != actual {
        return Err("Panda protocol packet/frame/reset/alert state differs".into());
    }
    Ok(())
}
struct Pending {
    report: Report,
    provenance: Publication,
    result: Publication,
    pins: ownership::Sources,
    executables: Vec<Bound>,
}
impl Pending {
    fn commit(mut self, flag: &AtomicBool) -> Result<Report> {
        self.pins.verify()?;
        for executable in &self.executables {
            executable.verify()?;
        }
        cancel(flag)?;
        self.provenance.commit()?;
        self.result.commit()?;
        Ok(self.report)
    }
}
fn execute(
    options: &Options,
    runner: &mut dyn Runner,
    stdout: &mut dyn Write,
    flag: &AtomicBool,
) -> Result<Pending> {
    if let Some(parent) = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let pins = ownership::sources(&options.root)?;
    let source_command = vec![resolve_non_strict(&options.source)?.into_os_string()];
    let mut native_command = Vec::new();
    if let (Some(qemu), Some(sysroot)) = (&options.qemu, &options.sysroot) {
        native_command.extend([
            resolve_non_strict(qemu)?.into_os_string(),
            "-L".into(),
            resolve_non_strict(sysroot)?.into_os_string(),
        ]);
    }
    native_command.push(resolve_non_strict(&options.binary)?.into_os_string());
    let source = Bound::open(Path::new(&source_command[0]));
    let binary = Bound::open(&resolve_non_strict(&options.binary)?);
    let source_argument = Bound::open(&options.source);
    let binary_argument = Bound::open(&options.binary);
    let executor = Bound::open(Path::new(&native_command[0]));
    let current = Bound::open(&std::env::current_exe()?)?;
    let encoded = run(
        runner,
        &source_command,
        &seeds(),
        &options.output,
        "seeds",
        flag,
    )?;
    let mut rows = packing();
    let pack_count = rows.len();
    rows.extend(decoding(&encoded)?);
    rows.extend(alerts());
    let expected = run(
        runner,
        &source_command,
        &rows,
        &options.output,
        "source",
        flag,
    )?;
    let valid_count = encoded.len() * 8 * 4 * 3;
    validate_flags(&expected, pack_count, valid_count)?;
    let actual = run(
        runner,
        &native_command,
        &rows,
        &options.output,
        "native",
        flag,
    )?;
    same(&expected, &actual)?;
    let mut invalid = Vec::new();
    for length in INVALID {
        let mut row =
            json!({"op":"pack","offset":0,"frames":[{"address":1,"src":0,"data":vec![0;length]}]});
        let mut exits = serde_json::Map::new();
        for (side, argv) in [("source", &source_command), ("native", &native_command)] {
            cancel(flag)?;
            let mut spec = CommandSpec::new_os(argv.clone(), Duration::from_secs(3));
            spec.input = Some(format!("{row}\n").into_bytes());
            spec.disable_core_dump = true;
            let child = runner.run(&spec)?;
            // Python text=True decodes both streams even when stdout is rejected.
            let stdout = String::from_utf8(child.stdout)?;
            let stderr = String::from_utf8(child.stderr)?
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            fs::write(
                options
                    .output
                    .join(format!("invalid-{length}-{side}.stderr")),
                stderr,
            )?;
            if child.code == 0 || !stdout.is_empty() {
                return Err(
                    format!("Panda protocol invalid length {length} accepted by {side}").into(),
                );
            }
            exits.insert(side.into(), json!(child.code));
        }
        invalid.push(json!({"length":length,"exits":exits}));
        row["frames"][0]["src"] = json!(4);
        let want = run(
            runner,
            &source_command,
            std::slice::from_ref(&row),
            &options.output,
            &format!("ignored-{length}-source"),
            flag,
        )?;
        let got = run(
            runner,
            &native_command,
            &[row],
            &options.output,
            &format!("ignored-{length}-native"),
            flag,
        )?;
        same(&want, &got)?;
        if want != [json!({"chunks":[]})] {
            return Err("protocol ignored bus emitted data".into());
        }
    }
    let source = source?;
    let binary = binary?;
    let source_argument = source_argument?;
    let binary_argument = binary_argument?;
    if source.hash != source_argument.hash || binary.hash != binary_argument.hash {
        return Err("protocol supplied path differs from executed probe".into());
    }
    let executor = executor?;
    let verify = || -> Result<()> {
        pins.verify()?;
        for bound in [
            &source,
            &binary,
            &source_argument,
            &binary_argument,
            &executor,
            &current,
        ] {
            bound.verify()?;
        }
        cancel(flag)
    };
    verify()?;
    let fields = json!({"result":"pass","cases":rows.len(),"pack_cases":pack_count,"decode_cases":rows.iter().filter(|r|r["op"]=="decode").count(),"alert_steps":rows.iter().map(|r|r["operations"].as_array().map_or(0,Vec::len)).sum::<usize>(),"valid_receive_flag_cases":valid_count,"invalid_lengths":invalid,"exact_packets_frames_resets_alert_state":true,"source_binary_sha256":source.hash,"native_binary_sha256":binary.hash,"scope":SCOPE});
    let report = Report {
        fields,
        native_command: command_json(&native_command)?,
    };
    let proof = json!({"source_sha256":pins.hashes(),"historical_checker_absent":pins.absent(),"historical_checker_sha256":ownership::PINS[0].1,"executing_checker_sha256":current.hash,"source_executable_sha256":source.hash,"native_runner_executable_sha256":executor.hash,"timeout_seconds":45,"invalid_timeout_seconds":3});
    // Existing original publishes its report before stdout; retain that success
    // contract while rolling owned receipts back if publication is interrupted.
    let provenance = Publication::write(&options.output.join("provenance.json"), &proof)?;
    let result = Publication::write(&options.output.join("report.json"), &report)?;
    verify()?;
    serde_json::to_writer(&mut *stdout, &report)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    verify()?;
    Ok(Pending {
        report,
        provenance,
        result,
        pins,
        executables: vec![
            source,
            binary,
            source_argument,
            binary_argument,
            executor,
            current,
        ],
    })
}
pub fn check(
    options: &Options,
    runner: &mut dyn Runner,
    stdout: &mut dyn Write,
    flag: &AtomicBool,
) -> Result<Report> {
    execute(options, runner, stdout, flag)?.commit(flag)
}
struct Controlled<'a>(&'a AtomicBool);
impl Runner for Controlled<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<crate::qa_process::Captured> {
        NativeRunner.run_controlled(spec, Instant::now() + spec.timeout, self.0)
    }
}
pub fn cli() -> Result<()> {
    let root = std::env::var_os("OPENPILOT_RUST_ROOT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        PathBuf::from,
    );
    let Some(options) = parse(&std::env::args_os().skip(1).collect::<Vec<_>>(), &root)? else {
        io::stdout().write_all(HELP.as_bytes())?;
        return Ok(());
    };
    let flag = Arc::new(AtomicBool::new(false));
    let mut handlers = Vec::new();
    let registration = (|| -> Result<()> {
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            handlers.push(signal_hook::flag::register(signal, Arc::clone(&flag))?);
        }
        Ok(())
    })();
    let result = registration
        .and_then(|()| execute(&options, &mut Controlled(&flag), &mut io::stdout(), &flag));
    for handler in handlers {
        signal_hook::low_level::unregister(handler);
    }
    cancel(&flag)?;
    result?.commit(&flag).map(|_| ())
}
