use openpilot_usbgpu::{
    check::{self, Options},
    Error,
};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
fn main() -> Result<(), Error> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let [devices, probe, timeout, clean] = arguments.as_slice() else {
        return Err(Error::Contract("expected devices probe timeout clean"));
    };
    let seconds = timeout
        .to_str()
        .ok_or(Error::Contract("timeout text"))?
        .parse::<f64>()
        .map_err(|_| Error::Contract("timeout"))?;
    let options = Options {
        devices: PathBuf::from(devices),
        probe: PathBuf::from(probe),
        timeout: Duration::try_from_secs_f64(seconds).map_err(|_| Error::Contract("timeout"))?,
        require_clean_link: clean == "1",
    };
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&stop))?;
    }
    println!(
        "{}",
        serde_json::json!({"error":check::run(&options,&stop)?})
    );
    Ok(())
}
