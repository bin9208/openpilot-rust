use std::{error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let source = PathBuf::from(args.next().ok_or("expected float32 input file")?);
    let output = PathBuf::from(args.next().ok_or("expected float32 output file")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let bytes = fs::read(source)?;
    if bytes.len() % 4 != 0 {
        return Err("input size is not a multiple of f32".into());
    }
    let mut result = Vec::with_capacity(bytes.len());
    for chunk in bytes.chunks_exact(4) {
        let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        result.extend_from_slice(&openpilot_modeld::parse::safe_exp(value).to_le_bytes());
    }
    fs::write(output, result)?;
    Ok(())
}
