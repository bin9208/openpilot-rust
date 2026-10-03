use openpilot_camera_kernel::{DoubleParseError, parse_double_prefix};
use serde_json::json;
use std::ffi::CString;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let value = std::env::args_os().nth(1).ok_or("parse input required")?;
    let value = CString::new(value.as_encoded_bytes())?;
    if std::fs::metadata("/camera-stress-owned-missing").is_ok() {
        return Err("owned nonexistent-path fixture exists".into());
    }
    let previous = std::io::Error::last_os_error().raw_os_error();
    let result = parse_double_prefix(&value);
    let after = std::io::Error::last_os_error().raw_os_error();
    let result = match result {
        Ok(value) => json!({"bits":value.to_bits()}),
        Err(DoubleParseError::InvalidArgument) => json!({"error":"invalid_argument"}),
        Err(DoubleParseError::OutOfRange) => json!({"error":"out_of_range"}),
    };
    println!("{}", json!({"previous_errno":previous,"after_errno":after,"result":result}));
    Ok(())
}
