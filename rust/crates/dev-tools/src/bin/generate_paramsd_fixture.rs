use openpilot_dev_tools::paramsd_fixture_generate::generate;
use std::path::PathBuf;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut root = None;
    let mut output = None;
    let mut saved_packet = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(option) = args.next() {
        let slot = match option.to_str() {
            Some("--root") => &mut root,
            Some("--output") => &mut output,
            Some("--saved-packet") => &mut saved_packet,
            _ => return Err("unknown option".into()),
        };
        if slot.is_some() {
            return Err("duplicate option".into());
        }
        *slot = Some(PathBuf::from(args.next().ok_or("option value missing")?));
    }
    generate(
        &root.ok_or("--root required")?,
        &output.ok_or("--output required")?,
        saved_packet.as_deref(),
    )
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("paramsd fixture generation: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
