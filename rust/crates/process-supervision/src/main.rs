use std::process::ExitCode;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or("missing launch descriptor")?;
    if args.next().is_some() {
        return Err("unexpected launch argument".into());
    }
    openpilot_process_supervision::run_child(std::fs::File::open(path)?)?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("native process launch failed: {error}");
            ExitCode::FAILURE
        }
    }
}
