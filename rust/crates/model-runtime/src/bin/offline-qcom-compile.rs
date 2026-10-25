use std::{
    io::{Read, Write},
    path::PathBuf,
};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let library = match args.next() {
        Some(key) if key == "--library" => {
            PathBuf::from(args.next().ok_or("--library requires a path")?)
        }
        _ => return Err(
            "usage: offline-qcom-compile --library PINNED_LIBLLVM_QCOM [--export-plan] < input > output"
                .into(),
        ),
    };
    let plan = match args.next() {
        None => false,
        Some(flag) if flag == "--export-plan" => true,
        _ => return Err("unexpected argument".into()),
    };
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    if plan {
        use openpilot_model_runtime::compiler_schedule::export::{prepare, Request, MAX_MESSAGE};
        let mut request = Vec::new();
        std::io::stdin()
            .take(MAX_MESSAGE as u64 + 1)
            .read_to_end(&mut request)?;
        if request.len() > MAX_MESSAGE {
            return Err("export-plan input exceeds bound".into());
        }
        let request: Request = serde_json::from_slice(&request)?;
        let output = serde_json::to_vec(&prepare(request).map_err(std::io::Error::other)?)?;
        if output.len() > MAX_MESSAGE {
            return Err("export-plan output exceeds bound".into());
        }
        std::io::stdout().write_all(&output)?;
        return Ok(());
    }
    let mut source = String::new();
    std::io::stdin()
        .take(64 * 1024 * 1024 + 1)
        .read_to_string(&mut source)?;
    let output =
        openpilot_model_runtime::qcom_compiler::Compiler::open(&library)?.compile(&source)?;
    std::io::stdout().write_all(&output)?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("offline QCOM compiler: {error}");
        std::process::exit(1);
    }
}
