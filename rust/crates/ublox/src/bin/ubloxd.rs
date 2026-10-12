use std::sync::{atomic::AtomicBool, Arc};
fn run() -> Result<(), openpilot_ublox::Error> {
    if std::env::args().nth(1).as_deref() == Some("--help") {
        println!("openpilot-ubloxd\nContinuous native ubloxRaw decoder and GNSS publisher.");
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    openpilot_ublox::runtime::decoder(&stop)
}

fn main() -> std::process::ExitCode {
    match openpilot_managed_entry::runtime("openpilot.system.ubloxd.ubloxd", "ubloxd", || {
        run().map_err(openpilot_managed_entry::StepError::raised)
    }) {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ubloxd: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
