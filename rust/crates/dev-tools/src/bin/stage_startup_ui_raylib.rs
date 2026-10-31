use openpilot_dev_tools::{
    raylib_stage::{self, Options},
    seal::Result,
};
use std::path::PathBuf;
fn run() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.iter().any(|arg| arg == "--prepare-native-inputs") {
        let arguments = arguments
            .into_iter()
            .map(|arg| {
                arg.into_string()
                    .map_err(|_| "non-UTF8 native raylib argument")
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        return openpilot_dev_tools::native_raylib_inputs::cli(&arguments);
    }
    let mut args = arguments.into_iter();
    let mut arch = None;
    let mut output = None;
    let mut cxx = "g++".to_owned();
    let mut wheel_file = None;
    let mut library_directories = Vec::new();
    let mut shared = false;
    while let Some(argument) = args.next() {
        match argument.to_str().ok_or("invalid argument")? {
            "--help" | "-h" => {
                println!("Usage: stage-startup-ui-raylib --arch x86_64|aarch64 --output PATH [--shared] [--cxx GNU_COMPILER] [--wheel-file PATH] [--library-directory PATH]...\n       stage-startup-ui-raylib --prepare-native-inputs --cache ASSET_CACHE --store PREFIX_STORE [--root SOURCE]\nPrepare only the original ARM headers/static archive and distribution metadata. No Python, compiler or runtime activation.");
                return Ok(());
            }
            "--arch" => {
                arch = Some(
                    args.next()
                        .ok_or("--arch needs a value")?
                        .into_string()
                        .map_err(|_| "invalid architecture")?,
                )
            }
            "--output" => {
                output = Some(PathBuf::from(args.next().ok_or("--output needs a value")?))
            }
            "--cxx" => {
                cxx = args
                    .next()
                    .ok_or("--cxx needs a value")?
                    .into_string()
                    .map_err(|_| "invalid compiler")?
            }
            "--wheel-file" => {
                wheel_file = Some(PathBuf::from(
                    args.next().ok_or("--wheel-file needs a value")?,
                ))
            }
            "--library-directory" => library_directories.push(PathBuf::from(
                args.next().ok_or("--library-directory needs a value")?,
            )),
            "--shared" => shared = true,
            _ => return Err("unknown argument".into()),
        }
    }
    let root = raylib_stage::stage(&Options {
        arch: arch.ok_or("--arch required")?,
        output: output.ok_or("--output required")?,
        shared,
        cxx,
        wheel_file,
        library_directories,
    })?;
    println!("STARTUP_UI_RAYLIB_ROOT={}", root.display());
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("raylib staging: {error}");
        std::process::exit(1);
    }
}
