use super::{Document, Text, Value};
use std::fmt::Write;
impl Text {
    fn write(&self, output: &mut String) -> std::fmt::Result {
        output.push('"');
        for &point in &self.0 {
            match point {
                8 => output.push_str("\\b"),
                9 => output.push_str("\\t"),
                10 => output.push_str("\\n"),
                12 => output.push_str("\\f"),
                13 => output.push_str("\\r"),
                34 => output.push_str("\\\""),
                92 => output.push_str("\\\\"),
                32..=126 => {
                    if let Some(character) = char::from_u32(point) {
                        output.push(character);
                    }
                }
                0..=0xffff => write!(output, "\\u{point:04x}")?,
                _ => {
                    let point = point - 0x10000;
                    write!(
                        output,
                        "\\u{:04x}\\u{:04x}",
                        0xd800 + (point >> 10),
                        0xdc00 + (point & 0x3ff)
                    )?;
                }
            }
        }
        output.push('"');
        Ok(())
    }
}
enum Part<'a> {
    Node(usize),
    Text(&'a Text),
    Token(&'static str),
}
impl Document {
    pub fn write(&self, output: &mut String) -> std::fmt::Result {
        let mut stack = vec![Part::Node(self.root)];
        while let Some(part) = stack.pop() {
            let node = match part {
                Part::Token(token) => {
                    output.push_str(token);
                    continue;
                }
                Part::Text(text) => {
                    text.write(output)?;
                    continue;
                }
                Part::Node(index) => &self.values[index],
            };
            match node {
                Value::Null => output.push_str("null"),
                Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
                Value::Integer(value) => output.push_str(value),
                Value::Float(value) => float(*value, output)?,
                Value::Text(text) => text.write(output)?,
                Value::Array(values) => {
                    output.push('[');
                    stack.push(Part::Token("]"));
                    for (index, &value) in values.iter().enumerate().rev() {
                        stack.push(Part::Node(value));
                        if index > 0 {
                            stack.push(Part::Token(", "));
                        }
                    }
                }
                Value::Object(values) => {
                    output.push('{');
                    stack.push(Part::Token("}"));
                    for (index, (key, value)) in values.iter().enumerate().rev() {
                        stack.push(Part::Node(*value));
                        stack.push(Part::Token(": "));
                        stack.push(Part::Text(key));
                        if index > 0 {
                            stack.push(Part::Token(", "));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
fn float(value: f64, output: &mut String) -> std::fmt::Result {
    if value.is_nan() {
        output.push_str("NaN");
        return Ok(());
    }
    if value.is_infinite() {
        output.push_str(if value.is_sign_positive() {
            "Infinity"
        } else {
            "-Infinity"
        });
        return Ok(());
    }
    // Schubfach's shortest even-tie digits match Python dtoa; Rust Debug
    // formatting chooses the other decimal on some exact halfway values.
    let mut buffer = zmij::Buffer::new();
    let text = buffer.format_finite(value);
    let unsigned = if let Some(text) = text.strip_prefix('-') {
        output.push('-');
        text
    } else {
        text
    };
    if value == 0. {
        output.push_str("0.0");
        return Ok(());
    }
    let (mantissa, exponent) = match unsigned.split_once('e') {
        Some((mantissa, exponent)) => (
            mantissa,
            exponent.parse::<i32>().map_err(|_| std::fmt::Error)?,
        ),
        None => (unsigned, 0),
    };
    let mut position = i32::try_from(mantissa.find('.').unwrap_or(mantissa.len()))
        .map_err(|_| std::fmt::Error)?
        + exponent;
    let digits: String = mantissa
        .chars()
        .filter(|&character| character != '.')
        .collect();
    let significant = digits.trim_start_matches('0');
    position -= i32::try_from(digits.len() - significant.len()).map_err(|_| std::fmt::Error)?;
    let significant = significant.trim_end_matches('0');
    let scientific = position - 1;
    if !(-4..16).contains(&scientific) {
        let (first, rest) = significant.split_at(1);
        output.push_str(first);
        if !rest.is_empty() {
            output.push('.');
            output.push_str(rest);
        }
        write!(output, "e{scientific:+03}")?;
    } else if position <= 0 {
        output.push_str("0.");
        output.extend(std::iter::repeat_n(
            '0',
            usize::try_from(-position).map_err(|_| std::fmt::Error)?,
        ));
        output.push_str(significant);
    } else {
        let position = usize::try_from(position).map_err(|_| std::fmt::Error)?;
        if position >= significant.len() {
            output.push_str(significant);
            output.extend(std::iter::repeat_n('0', position - significant.len()));
            output.push_str(".0");
        } else {
            let (first, rest) = significant.split_at(position);
            output.push_str(first);
            output.push('.');
            output.push_str(rest);
        }
    }
    Ok(())
}
