use openpilot_hardware_control::reboot::{self, Config};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut sound = false;
    let mut seconds = 0.0;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--sound-only" => sound = true,
            "--delay" => seconds = args.next().ok_or("missing delay")?.parse::<f64>()?,
            "--help" => {
                println!("openpilot-reboot [--delay SECONDS] [--sound-only]");
                return Ok(());
            }
            _ => return Err("unexpected reboot argument".into()),
        }
    }
    let config = Config::for_runtime()?;
    if sound {
        reboot::play_sound(&config)?;
    } else {
        config.device_only()?;
        std::thread::sleep(reboot::delay(seconds));
        reboot::reboot(&config)?;
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[reboot] {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
