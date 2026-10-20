use std::path::PathBuf;

fn run() -> openpilot_dev_tools::seal::Result<()> {
    let mut root = None;
    let mut output = None;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or("missing gesture generator flag value")?;
        match flag.to_str() {
            Some("--root") if root.is_none() => root = Some(PathBuf::from(value)),
            Some("--output") if output.is_none() => output = Some(PathBuf::from(value)),
            _ => {
                return Err(format!(
                    "unexpected gesture generator flag: {}",
                    flag.to_string_lossy()
                )
                .into())
            }
        }
    }
    let root = root.ok_or("missing --root")?;
    let output = output.ok_or("missing --output")?;
    openpilot_dev_tools::bluetooth_gesture_cases_generate::write(&root, &output)
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
