//! The source uses f-strings on unvalidated JSON values; keep Python str/repr semantics.
use crate::Error;
use openpilot_logmessaged::{JsonValue, JsonView};
use unicode_general_category::{get_general_category, GeneralCategory};

pub(crate) fn join(values: &[&JsonValue], separator: &str) -> Result<Vec<u32>, Error> {
    let mut output = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.extend(separator.chars().map(u32::from));
        }
        render(value, false, &mut output)?;
    }
    Ok(output)
}
fn append(output: &mut Vec<u32>, value: &str) {
    output.extend(value.chars().map(u32::from));
}
fn quoted(points: &[u32], output: &mut Vec<u32>) {
    let quote = if points.contains(&39) && !points.contains(&34) {
        34
    } else {
        39
    };
    output.push(quote);
    for &point in points {
        match point {
            9 => append(output, "\\t"),
            10 => append(output, "\\n"),
            13 => append(output, "\\r"),
            92 => append(output, "\\\\"),
            value if value == quote => {
                output.push(92);
                output.push(value);
            }
            value if printable(value) => output.push(value),
            0..=255 => append(output, &format!("\\x{point:02x}")),
            256..=65535 => append(output, &format!("\\u{point:04x}")),
            _ => append(output, &format!("\\U{point:08x}")),
        }
    }
    output.push(quote);
}
fn printable(point: u32) -> bool {
    if point == 32 {
        return true;
    }
    char::from_u32(point).is_some_and(|character| {
        !matches!(
            get_general_category(character),
            GeneralCategory::Control
                | GeneralCategory::Format
                | GeneralCategory::Surrogate
                | GeneralCategory::PrivateUse
                | GeneralCategory::Unassigned
                | GeneralCategory::LineSeparator
                | GeneralCategory::ParagraphSeparator
                | GeneralCategory::SpaceSeparator
        )
    })
}
fn render(value: &JsonValue, repr: bool, output: &mut Vec<u32>) -> Result<(), Error> {
    match value.view() {
        JsonView::Null => append(output, "None"),
        JsonView::Bool(value) => append(output, if value { "True" } else { "False" }),
        JsonView::Integer(value) => append(output, value),
        JsonView::Float(value) => {
            let mut text = String::new();
            openpilot_runtime_core::python_float::write_float(value, &mut text)?;
            append(
                output,
                &text.replace("NaN", "nan").replace("Infinity", "inf"),
            );
        }
        JsonView::Text(points) => {
            if repr {
                quoted(points, output);
            } else {
                output.extend_from_slice(points);
            }
        }
        JsonView::Array(values) => {
            output.push(91);
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    append(output, ", ");
                }
                render(value, true, output)?;
            }
            output.push(93);
        }
        JsonView::Object(values) => {
            output.push(123);
            for (index, (key, value)) in values.iter().enumerate() {
                if index > 0 {
                    append(output, ", ");
                }
                quoted(key, output);
                append(output, ": ");
                render(value, true, output)?;
            }
            output.push(125);
        }
    }
    Ok(())
}
