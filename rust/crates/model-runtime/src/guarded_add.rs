//! Conservative source rewrite for LLVM `fadd nsz` values guarded by a scalar
//! `select` with an explicit positive-zero arm.
//!
//! This source-rewrite candidate remains subject to hosted full-model
//! validation before any production use.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt;

pub const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RECEIPT_BYTES: usize = 32 * 1024 * 1024;

const ALLOWED_FAST_MATH_FLAGS: &[&str] = &["nsz", "arcp", "contract", "afn"];
const MAX_NAME_BYTES: usize = 1024;
const MAX_FUNCTIONS: usize = 16_384;
const MAX_SOURCE_LINES: usize = 262_144;
const MAX_SSA_NAMES_PER_FUNCTION: usize = 262_144;
const MAX_REWRITES: usize = 16_384;
const MAX_FLAGS: usize = ALLOWED_FAST_MATH_FLAGS.len();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteError {
    SourceTooLarge { bytes: usize },
    OutputTooLarge { bytes: usize },
    ReceiptTooLarge { bytes: usize },
    NameTooLarge { bytes: usize },
    TooManyFunctions { count: usize },
    TooManySourceLines { count: usize },
    TooManySsaNames { count: usize },
    TooManyRewrites { count: usize },
    GeneratedNameCollision { function: String, name: String },
}

impl fmt::Display for RewriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLarge { bytes } => write!(
                formatter,
                "LLVM source exceeds {} bytes: {bytes}",
                MAX_SOURCE_BYTES
            ),
            Self::OutputTooLarge { bytes } => write!(
                formatter,
                "rewritten LLVM source exceeds {} bytes: {bytes}",
                MAX_OUTPUT_BYTES
            ),
            Self::ReceiptTooLarge { bytes } => write!(
                formatter,
                "rewrite receipt exceeds {} bytes: {bytes}",
                MAX_RECEIPT_BYTES
            ),
            Self::NameTooLarge { bytes } => {
                write!(
                    formatter,
                    "LLVM identifier exceeds {MAX_NAME_BYTES} bytes: {bytes}"
                )
            }
            Self::TooManyFunctions { count } => {
                write!(formatter, "LLVM source has too many functions: {count}")
            }
            Self::TooManySourceLines { count } => {
                write!(formatter, "LLVM source has too many lines: {count}")
            }
            Self::TooManySsaNames { count } => {
                write!(formatter, "LLVM function has too many SSA names: {count}")
            }
            Self::TooManyRewrites { count } => {
                write!(
                    formatter,
                    "LLVM source has too many guarded-add rewrites: {count}"
                )
            }
            Self::GeneratedNameCollision { function, name } => {
                write!(
                    formatter,
                    "generated SSA name {name} collides in {function}"
                )
            }
        }
    }
}

impl std::error::Error for RewriteError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RewriteStatus {
    Unchanged,
    Rewritten,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RewriteRecord {
    pub function: String,
    pub line: usize,
    pub result_name: String,
    pub condition_name: String,
    pub input_name: String,
    pub selected_value_name: String,
    pub zero_arm_when_true: bool,
    pub generated_sum_name: String,
    pub removed_flag: String,
    pub retained_flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RewriteReceipt {
    pub status: RewriteStatus,
    pub original_source_sha256: String,
    pub output_source_sha256: String,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub rewrite_count: usize,
    pub rewrites: Vec<RewriteRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalizedSource {
    pub source: String,
    pub receipt: RewriteReceipt,
}

#[derive(Debug, Clone)]
struct SourceLine<'a> {
    full: &'a str,
    content: &'a str,
    ending: &'a str,
    function: Option<usize>,
}

#[derive(Debug, Default)]
struct FunctionInfo {
    name: String,
    ssa_names: HashSet<String>,
    selects: HashMap<String, Option<SelectInfo>>,
}

#[derive(Debug, Clone)]
struct SelectInfo {
    condition: String,
    value: String,
    zero_arm_when_true: bool,
}

#[derive(Debug)]
struct FaddInfo {
    result: String,
    flags: Vec<String>,
    left: String,
    right: String,
}

/// Rewrite only scalar fadds whose single selected operand has an explicit
/// positive-zero arm. Unsupported syntax is retained unchanged.
pub fn canonicalize(source: &str) -> Result<CanonicalizedSource, RewriteError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(RewriteError::SourceTooLarge {
            bytes: source.len(),
        });
    }

    let original_source_sha256 = sha256(source.as_bytes());
    let mut functions = Vec::<FunctionInfo>::new();
    let mut lines = Vec::<SourceLine<'_>>::new();
    let mut current_function = None;
    let mut pending_function = None;

    for (source_line_count, full) in source.split_inclusive('\n').enumerate() {
        if source_line_count >= MAX_SOURCE_LINES {
            return Err(RewriteError::TooManySourceLines {
                count: source_line_count + 1,
            });
        }
        let (content, ending) = split_line_ending(full);
        let trimmed = content.trim();

        if pending_function.is_none() && trimmed.starts_with("define ") {
            current_function = None;
            if functions.len() >= MAX_FUNCTIONS {
                return Err(RewriteError::TooManyFunctions {
                    count: functions.len() + 1,
                });
            }
            let id = functions.len();
            let mut function = FunctionInfo {
                name: function_name(trimmed, id)?,
                ..FunctionInfo::default()
            };
            collect_ssa_names(content, &mut function.ssa_names)?;
            functions.push(function);
            if content
                .split(';')
                .next()
                .unwrap_or("")
                .trim_end()
                .ends_with('{')
            {
                current_function = Some(id);
            } else {
                pending_function = Some(id);
            }
            lines.push(SourceLine {
                full,
                content,
                ending,
                function: None,
            });
            continue;
        }

        if let Some(id) = pending_function {
            collect_ssa_names(content, &mut functions[id].ssa_names)?;
            if content
                .split(';')
                .next()
                .unwrap_or("")
                .trim_end()
                .ends_with('{')
            {
                pending_function = None;
                current_function = Some(id);
            }
            lines.push(SourceLine {
                full,
                content,
                ending,
                function: None,
            });
            continue;
        }

        if let Some(id) = current_function {
            if trimmed.starts_with('}') {
                current_function = None;
                lines.push(SourceLine {
                    full,
                    content,
                    ending,
                    function: None,
                });
                continue;
            }

            collect_ssa_names(content, &mut functions[id].ssa_names)?;
            if let Some((result, rhs)) = assignment(content) {
                if rhs.starts_with("select ") {
                    let selected = parse_select(rhs);
                    match functions[id].selects.entry(result.to_owned()) {
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(selected);
                        }
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            entry.insert(None);
                        }
                    }
                }
            }
            lines.push(SourceLine {
                full,
                content,
                ending,
                function: Some(id),
            });
            continue;
        }

        lines.push(SourceLine {
            full,
            content,
            ending,
            function: None,
        });
    }

    let mut output = String::with_capacity(source.len());
    let mut rewrites = Vec::new();
    let mut receipt_estimated_bytes = 1024usize;
    for (line_index, line) in lines.iter().enumerate() {
        let candidate = line
            .function
            .and_then(|id| parse_fadd(line.content).map(|fadd| (id, fadd)));
        let Some((function_id, fadd)) = candidate else {
            push_bounded(&mut output, line.full)?;
            continue;
        };

        let function = &mut functions[function_id];
        let left_is_selected = function.selects.contains_key(&fadd.left);
        let right_is_selected = function.selects.contains_key(&fadd.right);
        if left_is_selected == right_is_selected {
            push_bounded(&mut output, line.full)?;
            continue;
        }

        let (selected_name, input_name) = if left_is_selected {
            (&fadd.left, &fadd.right)
        } else {
            (&fadd.right, &fadd.left)
        };
        let Some(Some(select)) = function.selects.get(selected_name) else {
            push_bounded(&mut output, line.full)?;
            continue;
        };
        if !fadd.flags.iter().any(|flag| flag == "nsz")
            || fadd
                .flags
                .iter()
                .filter(|flag| flag.as_str() == "nsz")
                .count()
                != 1
            || fadd
                .flags
                .iter()
                .any(|flag| !ALLOWED_FAST_MATH_FLAGS.contains(&flag.as_str()))
        {
            push_bounded(&mut output, line.full)?;
            continue;
        }

        let generated_sum_name = format!("%diagnostic_guarded_sum_{}", rewrites.len());
        let generated_zero_name = format!("{generated_sum_name}.zero");
        for name in [&generated_sum_name, &generated_zero_name] {
            if function.ssa_names.contains(name) {
                return Err(RewriteError::GeneratedNameCollision {
                    function: function.name.clone(),
                    name: name.clone(),
                });
            }
            function.ssa_names.insert(name.clone());
        }

        if rewrites.len() >= MAX_REWRITES {
            return Err(RewriteError::TooManyRewrites {
                count: rewrites.len() + 1,
            });
        }
        let retained_flags = fadd
            .flags
            .iter()
            .filter(|flag| flag.as_str() != "nsz")
            .cloned()
            .collect::<Vec<_>>();
        let record_size = estimate_record_size(
            &function.name,
            &fadd.result,
            &select.condition,
            input_name,
            &select.value,
            &generated_sum_name,
            &retained_flags,
        )?;
        let receipt_size = receipt_estimated_bytes
            .checked_add(record_size)
            .ok_or(RewriteError::ReceiptTooLarge { bytes: usize::MAX })?;
        if receipt_size > MAX_RECEIPT_BYTES {
            return Err(RewriteError::ReceiptTooLarge {
                bytes: receipt_size,
            });
        }
        receipt_estimated_bytes = receipt_size;

        let flags_text = if retained_flags.is_empty() {
            String::new()
        } else {
            format!("{} ", retained_flags.join(" "))
        };
        let input = input_name.as_str();
        let generated_sum = format!(
            "{}{} = fadd {}float {}, {}",
            leading_whitespace(line.content),
            generated_sum_name,
            flags_text,
            input,
            select.value
        );
        // Adding explicit +0 is observable: -0 + +0 becomes +0, and a
        // signaling NaN is quieted. Forwarding input bits loses that operation.
        // Keep this arm free of nsz and all other fast-math flags. The SSA name
        // derives from the receipt's generated_sum_name; both names are checked.
        let generated_zero = format!(
            "{}{} = fadd float {}, 0.0",
            leading_whitespace(line.content),
            generated_zero_name,
            input
        );
        let (true_value, false_value) = if select.zero_arm_when_true {
            (generated_zero_name.as_str(), generated_sum_name.as_str())
        } else {
            (generated_sum_name.as_str(), generated_zero_name.as_str())
        };
        let replacement = format!(
            "{}{} = select i1 {}, float {}, float {}",
            leading_whitespace(line.content),
            fadd.result,
            select.condition,
            true_value,
            false_value
        );

        let separator = if line.ending.is_empty() {
            "\n"
        } else {
            line.ending
        };
        push_bounded(&mut output, &generated_sum)?;
        push_bounded(&mut output, separator)?;
        push_bounded(&mut output, &generated_zero)?;
        push_bounded(&mut output, separator)?;
        push_bounded(&mut output, &replacement)?;
        push_bounded(&mut output, line.ending)?;

        rewrites.push(RewriteRecord {
            function: function.name.clone(),
            line: line_index + 1,
            result_name: fadd.result,
            condition_name: select.condition.clone(),
            input_name: input_name.clone(),
            selected_value_name: select.value.clone(),
            zero_arm_when_true: select.zero_arm_when_true,
            generated_sum_name,
            removed_flag: "nsz".to_owned(),
            retained_flags,
        });
    }

    let status = if rewrites.is_empty() {
        // Preserve the original bytes exactly when the conservative parser
        // finds no supported rewrite.
        output.clear();
        output.push_str(source);
        RewriteStatus::Unchanged
    } else {
        RewriteStatus::Rewritten
    };
    let output_source_sha256 = sha256(output.as_bytes());
    let receipt = RewriteReceipt {
        status,
        original_source_sha256,
        output_source_sha256,
        input_bytes: source.len(),
        output_bytes: output.len(),
        rewrite_count: rewrites.len(),
        rewrites,
    };
    Ok(CanonicalizedSource {
        source: output,
        receipt,
    })
}

fn split_line_ending(line: &str) -> (&str, &str) {
    if let Some(content) = line.strip_suffix('\n') {
        if let Some(content) = content.strip_suffix('\r') {
            (content, "\r\n")
        } else {
            (content, "\n")
        }
    } else {
        (line, "")
    }
}

fn assignment(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    let equals = trimmed.find('=')?;
    let result = trimmed[..equals].trim();
    let rhs = trimmed[equals + 1..].trim_start();
    is_ssa_name(result).then_some((result, rhs))
}

fn parse_select(rhs: &str) -> Option<SelectInfo> {
    let mut pieces = rhs.split(',');
    let condition = pieces.next()?.trim().strip_prefix("select i1 ")?.trim();
    let first = pieces.next()?.trim().strip_prefix("float ")?.trim();
    let second = pieces.next()?.trim().strip_prefix("float ")?.trim();
    if pieces.next().is_some() || !bounded_ssa_name(condition) {
        return None;
    }
    match (first, second) {
        ("0.0", value) if bounded_ssa_name(value) => Some(SelectInfo {
            condition: condition.to_owned(),
            value: value.to_owned(),
            zero_arm_when_true: true,
        }),
        (value, "0.0") if bounded_ssa_name(value) => Some(SelectInfo {
            condition: condition.to_owned(),
            value: value.to_owned(),
            zero_arm_when_true: false,
        }),
        _ => None,
    }
}

fn parse_fadd(line: &str) -> Option<FaddInfo> {
    let (result, rhs) = assignment(line)?;
    let rest = rhs.strip_prefix("fadd ")?;
    let float_pos = rest.find("float ")?;
    let mut flags = Vec::new();
    for flag in rest[..float_pos].split_whitespace() {
        if flags.len() == MAX_FLAGS || flag.len() > 32 {
            return None;
        }
        flags.push(flag.to_owned());
    }
    if flags
        .iter()
        .any(|flag| !ALLOWED_FAST_MATH_FLAGS.contains(&flag.as_str()))
    {
        return None;
    }
    let operands = &rest[float_pos + "float ".len()..];
    let (left, right) = operands.split_once(',')?;
    let left = left.trim();
    let right = right.trim();
    if right.contains(',') || !bounded_ssa_name(left) || !bounded_ssa_name(right) {
        return None;
    }
    Some(FaddInfo {
        result: result.to_owned(),
        flags,
        left: left.to_owned(),
        right: right.to_owned(),
    })
}

fn is_ssa_name(name: &str) -> bool {
    let Some(body) = name.strip_prefix('%') else {
        return false;
    };
    !body.is_empty()
        && body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-.$_".contains(&byte))
}

fn bounded_ssa_name(name: &str) -> bool {
    name.len() <= MAX_NAME_BYTES && is_ssa_name(name)
}

fn leading_whitespace(line: &str) -> &str {
    let end = line
        .char_indices()
        .find(|(_, character)| !matches!(character, ' ' | '\t'))
        .map_or(line.len(), |(index, _)| index);
    &line[..end]
}

fn function_name(header: &str, ordinal: usize) -> Result<String, RewriteError> {
    let name_end = header.find('(').unwrap_or(header.len());
    let Some(at) = header[..name_end].rfind('@') else {
        return Ok(format!("function_{ordinal}"));
    };
    let tail = &header[at + 1..name_end];
    if let Some(quoted) = tail.strip_prefix('"') {
        if let Some(end) = quoted.find('"') {
            let name = &quoted[..end];
            if name.len() > MAX_NAME_BYTES {
                return Err(RewriteError::NameTooLarge { bytes: name.len() });
            }
            return Ok(name.to_owned());
        }
    }
    let name = tail.trim();
    if name.is_empty() {
        Ok(format!("function_{ordinal}"))
    } else {
        if name.len() > MAX_NAME_BYTES {
            return Err(RewriteError::NameTooLarge { bytes: name.len() });
        }
        Ok(name.to_owned())
    }
}

fn collect_ssa_names(line: &str, names: &mut HashSet<String>) -> Result<(), RewriteError> {
    // Block labels share the LLVM local namespace with SSA values.
    let code = line.split(';').next().unwrap_or("").trim();
    if let Some(label) = code.strip_suffix(':') {
        if let Some(raw) = label.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            if raw.len() > MAX_NAME_BYTES * 3 {
                return Err(RewriteError::NameTooLarge { bytes: raw.len() });
            }
            if let Ok(name) = decode_quoted_name(raw) {
                insert_ssa_name(names, name)?;
            }
        } else if !label.is_empty() && label.bytes().all(is_ssa_byte) {
            if label.len() > MAX_NAME_BYTES {
                return Err(RewriteError::NameTooLarge { bytes: label.len() });
            }
            insert_ssa_name(names, label.to_owned())?;
        }
    }
    let bytes = line.as_bytes();
    let mut index = 0;
    let mut in_string = false;
    while index < bytes.len() {
        if in_string {
            if bytes[index] == b'\\' {
                index = (index + 3).min(bytes.len());
                continue;
            }
            if bytes[index] == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match bytes[index] {
            b';' => break,
            b'"' => {
                in_string = true;
                index += 1;
            }
            b'%' => {
                if bytes.get(index + 1) == Some(&b'"') {
                    if let Some(end) = bytes[index + 2..].iter().position(|byte| *byte == b'"') {
                        let raw = &line[index + 2..index + 2 + end];
                        if raw.len() > MAX_NAME_BYTES * 3 {
                            return Err(RewriteError::NameTooLarge { bytes: raw.len() });
                        }
                        if let Ok(decoded) = decode_quoted_name(raw) {
                            insert_ssa_name(names, decoded)?;
                        }
                        index += end + 3;
                    } else {
                        index += 1;
                    }
                } else {
                    let start = index;
                    index += 1;
                    while index < bytes.len() && is_ssa_byte(bytes[index]) {
                        index += 1;
                    }
                    if index > start + 1 {
                        let name = &line[start..index];
                        if name.len() > MAX_NAME_BYTES + 1 {
                            return Err(RewriteError::NameTooLarge { bytes: name.len() });
                        }
                        insert_ssa_name(names, name[1..].to_owned())?;
                    }
                }
            }
            _ => index += 1,
        }
    }
    Ok(())
}

fn insert_ssa_name(names: &mut HashSet<String>, body: String) -> Result<(), RewriteError> {
    if body.len() > MAX_NAME_BYTES {
        return Err(RewriteError::NameTooLarge { bytes: body.len() });
    }
    if names.len() >= MAX_SSA_NAMES_PER_FUNCTION && !names.contains(&format!("%{body}")) {
        return Err(RewriteError::TooManySsaNames {
            count: names.len() + 1,
        });
    }
    names.insert(format!("%{body}"));
    Ok(())
}

fn estimate_record_size(
    function: &str,
    result: &str,
    condition: &str,
    input: &str,
    selected_value: &str,
    generated: &str,
    flags: &[String],
) -> Result<usize, RewriteError> {
    let fields = [
        function,
        result,
        condition,
        input,
        selected_value,
        generated,
        "nsz",
    ];
    let escaped = fields.iter().try_fold(512usize, |size, field| {
        field
            .len()
            .checked_mul(6)
            .and_then(|field_size| size.checked_add(field_size))
            .ok_or(RewriteError::ReceiptTooLarge { bytes: usize::MAX })
    })?;
    let escaped_flags = flags.iter().try_fold(escaped, |size, flag| {
        flag.len()
            .checked_mul(6)
            .and_then(|field_size| size.checked_add(field_size))
            .ok_or(RewriteError::ReceiptTooLarge { bytes: usize::MAX })
    })?;
    Ok(escaped_flags)
}

fn decode_quoted_name(raw: &str) -> Result<String, ()> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            if index + 2 >= bytes.len() {
                return Err(());
            }
            let high = hex_value(bytes[index + 1]).ok_or(())?;
            let low = hex_value(bytes[index + 2]).ok_or(())?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| ())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_ssa_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-.$_".contains(&byte)
}

fn push_bounded(output: &mut String, value: &str) -> Result<(), RewriteError> {
    let bytes = output
        .len()
        .checked_add(value.len())
        .ok_or(RewriteError::OutputTooLarge { bytes: usize::MAX })?;
    if bytes > MAX_OUTPUT_BYTES {
        return Err(RewriteError::OutputTooLarge { bytes });
    }
    output.push_str(value);
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod bounds_tests {
    use super::*;
    #[test]
    fn output_limit_prevents_append_instead_of_truncating() {
        let mut output = " ".repeat(MAX_OUTPUT_BYTES);
        assert!(matches!(
            push_bounded(&mut output, "x"),
            Err(RewriteError::OutputTooLarge { .. })
        ));
        assert_eq!(output.len(), MAX_OUTPUT_BYTES);
        assert!(output.as_bytes().iter().all(|b| *b == b' '));
    }
    #[test]
    fn ssa_limit_prevents_new_name_but_allows_existing_name() {
        let mut names = (0..MAX_SSA_NAMES_PER_FUNCTION)
            .map(|i| format!("%n{i}"))
            .collect::<HashSet<_>>();
        insert_ssa_name(&mut names, "n0".into()).unwrap();
        assert!(matches!(
            insert_ssa_name(&mut names, "new".into()),
            Err(RewriteError::TooManySsaNames { .. })
        ));
        assert!(!names.contains("%new"));
    }
}
