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
            "usage: offline-qcom-compile --library PINNED_LIBLLVM_QCOM < kernel.cl > kernel.bin"
                .into(),
        ),
    };
    if args.next().is_some() {
        return Err("unexpected argument".into());
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
