use openpilot_dev_tools::qcom_compiler_stage::{stage, Providers};
use std::path::PathBuf;
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let (output, binary, compiler, qemu, libraries) = match args.as_slice() {
        [flag, output, binary, compiler, libraries] if flag == std::path::Path::new("--native") => {
            (output, binary, compiler, None, libraries)
        }
        [output, binary, compiler, qemu, libraries] => {
            (output, binary, compiler, Some(qemu.clone()), libraries)
        }
        _ => return Err(
            "usage: stage-qcom-compiler [--native] OUTPUT ARM_FRONTEND LIBLLVM_QCOM [QEMU] LIB_DIR"
                .into(),
        ),
    };
    println!(
        "{}",
        stage(
            output,
            &Providers {
                binary: binary.clone(),
                compiler: compiler.clone(),
                qemu,
                libraries: libraries.clone()
            }
        )?["command"]
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("stage QCOM compiler: {error}");
        std::process::exit(1);
    }
}
