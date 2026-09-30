use std::fmt::Write;
pub fn write_float(value: f64, output: &mut String) -> std::fmt::Result {
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
