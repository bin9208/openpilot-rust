//! Compare native control tool states with preserved original-source fixture outputs.
mod json;
use crate::{
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path, time::Duration};
pub fn check(
    binary: &Path,
    source: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    let request = fs::read(source.join("input.json"))?;
    let expected = json::parse(&fs::read(source.join("expected.json"))?)?;
    let resolved = resolve_non_strict(binary)?;
    let mut command = CommandSpec::new(
        vec![resolved.to_str().ok_or("non-Unicode binary path")?.into()],
        Duration::from_secs(300),
    );
    command.input = Some(request.clone());
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    let result = runner.run(&command)?;
    fs::write(output.join("stdout.json"), &result.stdout)?;
    fs::write(output.join("stderr.log"), &result.stderr)?;
    if result.code != 0 {
        return Err(format!("control policy probe exited {}", result.code).into());
    }
    let actual = json::parse(&result.stdout)?;
    let expected = json::sequence(&expected)?;
    let actual = json::sequence(&actual)?;
    if actual.len() != expected.len() {
        return Err("control policy case counts differ".into());
    }
    let mut mismatches = vec![];
    let mut count = 0;
    let mut steps = 0;
    for (case, (left, right)) in expected.iter().zip(&actual).enumerate() {
        let left = json::sequence(left)?;
        let right = json::sequence(right)?;
        if left.len() != right.len() {
            return Err(format!("control policy step counts differ at case {case}").into());
        }
        steps += right.len();
        for (step, (a, b)) in left.iter().zip(&right).enumerate() {
            if !json::equal(a, b) {
                count += 1;
                if mismatches.len() < 30 {
                    mismatches.push((case, step, a.clone(), b.clone()));
                }
            }
        }
    }
    let binary_hash = format!("{:x}", Sha256::digest(fs::read(binary)?));
    let input_hash = format!("{:x}", Sha256::digest(&request));
    // Retain decoded nodes through serialization. JSON escape text is not an
    // injective representation of Python strings: reparsing a raw surrogate
    // pair can combine it and even collapse two distinct object keys.
    let mut compact = Vec::new();
    let mut expanded = Vec::new();
    for (case, step, a, b) in &mismatches {
        compact.push(format!(
            "{{\"case\": {case}, \"step\": {step}, \"expected\": {}, \"actual\": {}}}",
            a.to_json()?,
            b.to_json()?
        ));
        expanded.push(format!("    {{\n      \"case\": {case},\n      \"step\": {step},\n      \"expected\": {},\n      \"actual\": {}\n    }}", json::pretty(a, 3)?, json::pretty(b, 3)?));
    }
    let pretty_mismatches = if expanded.is_empty() {
        "[]".into()
    } else {
        format!("[\n{}\n  ]", expanded.join(",\n"))
    };
    let executable = JsonValue::text(&command.argv[0]).to_json()?;
    let pretty = format!("{{\n  \"cases\": {},\n  \"steps\": {steps},\n  \"mismatch_count\": {count},\n  \"mismatches\": {pretty_mismatches},\n  \"binary_sha256\": \"{binary_hash}\",\n  \"input_sha256\": \"{input_hash}\",\n  \"command\": [\n    {executable}\n  ]\n}}\n", actual.len());
    fs::write(output.join("receipt.json"), pretty)?;
    writeln!(printed, "{{\"cases\": {}, \"steps\": {steps}, \"mismatch_count\": {count}, \"mismatches\": [{}], \"binary_sha256\": \"{binary_hash}\", \"input_sha256\": \"{input_hash}\", \"command\": [{executable}]}}", actual.len(), compact.join(", "))?;
    if count != 0 {
        return Err("control tool source states differ".into());
    }
    Ok(())
}

pub fn cli() -> Result<()> {
    use crate::{
        argparse_scan::{scan, OptionSpec},
        qa_process::NativeRunner,
    };
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let parsed = scan(
        &args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--source", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        println!(
            "Usage: check-control-tools-policy --binary PATH --source DIRECTORY --output DIRECTORY"
        );
        return Ok(());
    }
    check(
        Path::new(&parsed.values["--binary"]),
        Path::new(&parsed.values["--source"]),
        Path::new(&parsed.values["--output"]),
        &mut NativeRunner,
        &mut std::io::stdout().lock(),
    )
}
