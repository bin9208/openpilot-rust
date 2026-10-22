//! Source/runtime comparison orchestration; native peer owns each isolated process.
use crate::{control_tools_policy_check::json, path_resolve::resolve_non_strict, seal::Result};
use openpilot_log_format::JsonValue;
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
pub const CASES: [&str; 10] = [
    "main",
    "--stream",
    "--carrot-vision-road",
    "--youtube-low",
    "--youtube-medium",
    "--youtube",
    "--youtube-wide",
    "main-multi",
    "main-restart",
    "main-lag",
];
pub struct Options {
    pub source: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
    pub native: PathBuf,
    pub cases: Vec<String>,
}
pub trait Peers {
    fn create(
        &mut self,
        binary: &Path,
        directory: &Path,
        case: &str,
        native: &Path,
        cpus: &[usize],
    ) -> Result<()>;
    fn exercise(&mut self, kind: &str, case: &str) -> Result<()>;
    fn close(&mut self, kind: &str, case: &str) -> Result<()>;
    fn rows(&self, kind: &str) -> Result<Value>;
}
fn write_pretty(path: &Path, text: &str) -> Result<()> {
    let v = json::parse_text(text)?;
    fs::write(path, json::pretty(&v, 0)? + "\n")?;
    Ok(())
}
pub fn execute(
    options: &Options,
    cpus: &[usize],
    free: u64,
    peers: &mut dyn Peers,
    stdout: &mut dyn Write,
) -> Result<()> {
    let output = resolve_non_strict(&options.output)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&output)?;
    if free < 1024 * 1024 * 1024 {
        return Err("runtime capture requires 1 GiB operation growth".into());
    }
    let cases = if options.cases.is_empty() {
        CASES.iter().map(|v| (*v).to_owned()).collect()
    } else {
        options.cases.clone()
    };
    let mut results = vec![];
    for (index, case) in cases.iter().enumerate() {
        // Original resolves both candidates before entering either peer for each case.
        let binaries = [
            resolve_non_strict(&options.source)?,
            resolve_non_strict(&options.binary)?,
        ];
        for (kind, binary) in ["source", "rust"].into_iter().zip(binaries) {
            peers.create(
                &binary,
                &output.join(format!("case-{index}-{kind}")),
                case,
                &resolve_non_strict(&options.native)?,
                cpus,
            )?;
            let exercised = peers.exercise(kind, case);
            peers.close(kind, case)?;
            exercised?;
        }
        let source = peers.rows("source")?;
        let native = peers.rows("rust")?;
        if source != native {
            return Err(
                format!("{case}: source/Rust publications differ; inspect packets.json").into(),
            );
        }
        let rows = source.as_object().ok_or("encoder rows must be an object")?;
        let mut ordered = crate::encoder_runtime_exercise::services(case);
        ordered.extend(
            rows.keys()
                .map(String::as_str)
                .filter(|s| !crate::encoder_runtime_exercise::services(case).contains(s)),
        );
        let counts = ordered
            .into_iter()
            .filter_map(|s| rows.get(s).map(|v| (s, v)))
            .map(|(s, v)| {
                Ok(format!(
                    "{}: {}",
                    JsonValue::text(s).to_json()?,
                    v.as_array().ok_or("encoder rows must be arrays")?.len()
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        let result=format!("{{\"case\": {}, \"packets_per_process\": {{{counts}}}, \"exact_bytes_and_metadata\": true}}",JsonValue::text(case).to_json()?);
        results.push(result.clone());
        write_pretty(
            &output.join("progress.json"),
            &format!("[{}]", results.join(",")),
        )?;
        writeln!(stdout, "{result}")?;
        stdout.flush()?;
    }
    write_pretty(&output.join("receipt.json"),&format!("{{\"status\":\"PASS\",\"results\":[{}],\"scope\":\"host PC codecs and real VisionIPC/message queues; not AGNOS hardware or CPU performance\"}}",results.join(",")))
}
