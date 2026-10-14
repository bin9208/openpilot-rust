//! Compare the unchanged filter trace against captured original-source outputs.
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};

const HEADER: &str = "case\tstep\tx\trc\tdt\tinitialized\tfirst\tbounce";
const REFERENCE: &[u8] = include_bytes!("../data/filter-reference.f64le");
const PROVENANCE: &str = include_str!("../data/filter-reference.json");

pub struct Report {
    pub outputs: usize,
    pub max_abs_error: f64,
}

pub fn compare(trace: &str, reference: &[u8], input_sha256: &str) -> Result<Report> {
    if reference.is_empty() || !reference.len().is_multiple_of(16) {
        return Err("filter reference must contain pairs of float64 outputs".into());
    }
    let mut lines = trace.lines();
    if lines.next() != Some(HEADER) {
        return Err("unexpected filter trace header".into());
    }
    let mut inputs = Sha256::new();
    let mut report = Report {
        outputs: 0,
        max_abs_error: 0.0,
    };
    for (row, line) in lines.enumerate() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 8 {
            return Err(format!("filter trace row {row} must contain eight fields").into());
        }
        for (index, field) in fields[..6].iter().enumerate() {
            if index != 0 {
                inputs.update(b"\t");
            }
            inputs.update(field.as_bytes());
        }
        inputs.update(b"\n");
        for (field, name) in fields[6..].iter().zip(["first", "bounce"]) {
            let offset = report
                .outputs
                .checked_mul(8)
                .ok_or("filter output offset overflow")?;
            let expected = reference
                .get(offset..offset + 8)
                .ok_or("extra filter trace outputs")?;
            let expected = f64::from_le_bytes(expected.try_into()?);
            let actual: f64 = field.parse()?;
            if !expected.is_finite() || !actual.is_finite() {
                return Err(format!("nonfinite filter output at row {row} {name}").into());
            }
            let error = (actual - expected).abs();
            // Original math.isclose(rel_tol=1e-12, abs_tol=1e-12).
            if error > 1e-12 && error > 1e-12 * actual.abs().max(expected.abs()) {
                return Err(format!(
                    "filter mismatch at row {row} {name}: expected {expected}, got {actual}"
                )
                .into());
            }
            report.max_abs_error = report.max_abs_error.max(error);
            report.outputs += 1;
        }
    }
    if report.outputs != reference.len() / 8 {
        return Err("missing filter trace outputs".into());
    }
    if format!("{:x}", inputs.finalize()) != input_sha256 {
        return Err("filter input corpus changed; original reference no longer applies".into());
    }
    Ok(report)
}

pub fn check(binary: &Path) -> Result<Report> {
    let provenance: serde_json::Value = serde_json::from_str(PROVENANCE)?;
    if provenance["reference_sha256"] != format!("{:x}", Sha256::digest(REFERENCE))
        || provenance["outputs"] != 24_000
        || REFERENCE.len() != 24_000 * 8
    {
        return Err("filter original-source reference identity mismatch".into());
    }
    let binary = binary.canonicalize()?;
    let captured = NativeRunner.run(&CommandSpec::new(
        vec![binary
            .to_str()
            .ok_or("filter binary path is not UTF-8")?
            .into()],
        Duration::from_secs(30),
    ))?;
    if captured.code != 0 {
        return Err(format!(
            "filter trace failed ({}): {}",
            captured.code,
            String::from_utf8_lossy(&captured.stderr)
        )
        .into());
    }
    compare(
        std::str::from_utf8(&captured.stdout)?,
        REFERENCE,
        provenance["input_sha256"]
            .as_str()
            .ok_or("filter input digest missing")?,
    )
}
