use openpilot_lagd::{
    estimator::Estimator,
    loop_state::{self, TOPICS},
    message,
    parameters::{self, Writer},
    settings::Settings,
    wire, Error,
};
use openpilot_logging::producer::Factory;
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::{Options, Poll},
};
use openpilot_params::Params;
use std::{
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
fn run() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let mut remaining = match args.next().as_deref() {
        None => None,
        Some("--help") => {
            println!("openpilot-lagd [--frames N]\nNative lateral lag estimator; normal Params/msgq paths and SIGINT/SIGTERM shutdown.\n--frames bounds publications for host validation; production selection is unchanged.");
            return Ok(());
        }
        Some("--frames") => Some(
            args.next()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .ok_or(Error::Contract("positive frame count required"))?,
        ),
        Some(_) => return Err(Error::Contract("unknown argument")),
    };
    if args.next().is_some() {
        return Err(Error::Contract("unexpected argument"));
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    openpilot_torqued::platform::configure()?;
    let debug = std::env::var("DEBUG")
        .unwrap_or_else(|_| "0".into())
        .trim()
        .parse::<i64>()
        .map_err(|_| Error::Contract("DEBUG must be an integer"))?
        != 0;
    let mut publisher = PubMaster::for_runtime(&["liveDelay"])?;
    let mut subscriber = SubMaster::for_runtime(
        &TOPICS,
        Options {
            poll: Poll::One("livePose".into()),
            ..Options::default()
        },
    )?;
    let params = Params::for_runtime()?;
    let car = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if let Some(bytes) = parameters::read(&params, "CarParams")? {
            break wire::car(&bytes)?;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let frequency = openpilot_messaging::services::lookup("livePose")
        .ok_or(Error::Contract("livePose service missing"))?
        .frequency;
    let mut estimator = Estimator::new(
        Settings {
            dt: 1. / frequency,
            ..Settings::default()
        },
        car.actuator_delay,
    )?;
    let mut logger = Factory::for_runtime()?.logger();
    if let Some((lag, blocks)) = parameters::retrieve(&params, &car, &mut logger)? {
        estimator.reset(lag, blocks)?;
    }
    let writer = Writer::new(Params::for_runtime()?)?;
    while !stop.load(Ordering::Relaxed) {
        subscriber.update(Duration::from_millis(1000))?;
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let action = loop_state::step(&mut estimator, &subscriber.state)?;
        if action.publish {
            let bytes = wire::encode(
                &message::packet(&estimator, debug)?,
                openpilot_torqued::platform::timestamp()?,
                action.valid,
            )?;
            publisher.send("liveDelay", &bytes)?;
            if action.persist {
                writer.put(bytes)?;
            }
            if let Some(count) = &mut remaining {
                *count -= 1;
                if *count == 0 {
                    break;
                }
            }
        }
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lagd: {error}");
            ExitCode::FAILURE
        }
    }
}
