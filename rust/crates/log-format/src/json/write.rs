use super::{Document, Text, Value};
use openpilot_runtime_core::python_float::write_float as float;
use std::fmt::Write;

#[derive(Clone, Copy)]
pub(super) enum TextEncoding {
    AsciiEscaped,
    Utf8,
}

impl Text {
    fn write(&self, output: &mut String, encoding: TextEncoding) -> std::fmt::Result {
        let utf8 = match encoding {
            TextEncoding::AsciiEscaped => false,
            TextEncoding::Utf8 => true,
        };
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
                point if utf8 && point >= 127 => {
                    output.push(char::from_u32(point).ok_or(std::fmt::Error)?);
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
        self.write_node(self.root, output, TextEncoding::AsciiEscaped)
    }
    pub(super) fn write_node(
        &self,
        root: usize,
        output: &mut String,
        encoding: TextEncoding,
    ) -> std::fmt::Result {
        let mut stack = vec![Part::Node(root)];
        while let Some(part) = stack.pop() {
            let node = match part {
                Part::Token(token) => {
                    output.push_str(token);
                    continue;
                }
                Part::Text(text) => {
                    text.write(output, encoding)?;
                    continue;
                }
                Part::Node(index) => &self.values[index],
            };
            match node {
                Value::Null => output.push_str("null"),
                Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
                Value::Integer(value) => output.push_str(value),
                Value::Float(value) => float(*value, output)?,
                Value::Text(text) => text.write(output, encoding)?,
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
