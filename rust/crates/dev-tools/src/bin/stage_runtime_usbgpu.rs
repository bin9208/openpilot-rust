use openpilot_dev_tools::usbgpu_stage::{stage, Bundle};
use std::path::PathBuf;
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut root = None;
    let mut package = None;
    let mut identity = None;
    let mut license = None;
    let mut model = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(option) = args.next() {
        let value = args.next().ok_or("missing argument value")?;
        match option.to_str() {
            Some("--root") => root = Some(PathBuf::from(value)),
            Some("--package") => package = Some(PathBuf::from(value)),
            Some("--expect-manifest-sha256") => {
                identity = Some(
                    value
                        .into_string()
                        .map_err(|_| "invalid manifest identity")?,
                )
            }
            Some("--firmware-license") => license = Some(PathBuf::from(value)),
            Some("--model") => model = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown option: {}", option.to_string_lossy()).into()),
        }
    }
    let root = root.ok_or("--root required")?.canonicalize()?;
    let bundle = Bundle {
        path: package.ok_or("--package required")?.canonicalize()?,
        manifest_sha256: identity.ok_or("--expect-manifest-sha256 required")?,
        firmware_license: license
            .ok_or("--firmware-license required")?
            .canonicalize()?,
        model: model.map(|path| path.canonicalize()).transpose()?,
    };
    println!("{}", stage(&root, &bundle)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("stage eGPU companions: {error}");
        std::process::exit(1);
    }
}
