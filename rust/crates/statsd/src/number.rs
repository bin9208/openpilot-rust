use crate::Error;
pub(crate) fn repr(value: f64) -> Result<String, Error> {
    let mut output = String::new();
    openpilot_runtime_core::python_float::write_float(value, &mut output)?;
    Ok(output.replace("NaN", "nan").replace("Infinity", "inf"))
}
pub(crate) fn sum(values: &[f64]) -> f64 {
    let mut result = 0.0 + values[0];
    let mut correction = 0.0;
    for &value in &values[1..] {
        let next = result + value;
        correction += if result.abs() >= value.abs() {
            (result - next) + value
        } else {
            (value - next) + result
        };
        result = next;
    }
    if correction != 0.0 && correction.is_finite() {
        result += correction;
    }
    result
}
pub(crate) use openpilot_runtime_core::python_float::parse;
