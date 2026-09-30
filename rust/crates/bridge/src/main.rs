use std::{
    env,
    sync::{atomic::AtomicBool, Arc},
};

fn main() -> Result<(), openpilot_bridge::Error> {
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        libc::SIGPWR,
    ] {
        signal_hook::flag::register(signal, stop.clone())?;
    }
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() > 1 {
        openpilot_bridge::incoming::run(
            &openpilot_bridge::services(Some(&args[1])),
            &args[0],
            &stop,
        )
    } else {
        openpilot_bridge::outgoing::run(&openpilot_bridge::services(None), &stop)
    }
}
