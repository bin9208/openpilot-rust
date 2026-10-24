//! Validate owned hardware command fixture output and inherited stderr.
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn equals_python_number(value: &JsonValue, expected: u64) -> bool {
    match value.view() {
        JsonView::Bool(value) => u64::from(value) == expected,
        JsonView::Integer(value) => value == expected.to_string(),
        JsonView::Float(value) => value == expected as f64,
        _ => false,
    }
}
pub fn check(evidence: &Path) -> Result<()> {
    let receipt = JsonValue::parse(&fs::read_to_string(evidence.join("command.json"))?)?;
    let fields = match receipt.view() {
        JsonView::Object(fields) => fields,
        _ => return Err("hardware command JSON must equal the expected object".into()),
    };
    let expected = [
        ("status", 7),
        ("stdout_bytes", 131072),
        ("stdout_zero", 1),
        ("call_status", 3),
        ("shell_status", 5),
        ("missing_is_io", 1),
    ];
    if fields.len() != expected.len()
        || expected.into_iter().any(|(key, value)| {
            !receipt
                .get(key)
                .is_some_and(|actual| equals_python_number(&actual, value))
        })
    {
        return Err("hardware command JSON differs from expected result".into());
    }
    // Do not trim: the original assertion rejects even one trailing newline.
    if fs::read_to_string(evidence.join("command.stderr"))? != "inherited-stderr" {
        return Err("hardware command inherited stderr differs".into());
    }
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--help"] || args == ["-h"] {
        println!("Usage: check-hardware-command --evidence DIRECTORY");
        return Ok(());
    }
    let evidence = match args.as_slice() {
        [key, path] if key == "--evidence" => PathBuf::from(path),
        _ => return Err("expected --evidence DIRECTORY".into()),
    };
    check(&evidence)
}
