use std::sync::{atomic::AtomicBool, Arc};
fn main() -> Result<(), openpilot_ublox::Error> {
    if std::env::args().nth(1).as_deref() == Some("--help") {
        println!("openpilot-ubloxd\nContinuous native ubloxRaw decoder and GNSS publisher.");
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    openpilot_ublox::runtime::decoder(&stop)
}
