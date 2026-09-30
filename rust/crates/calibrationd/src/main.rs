use openpilot_calibrationd::{
    loop_state,
    parameters::{self, PendingWrites},
    platform, wire, Calibrator, Error, Seed,
};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::{Options, Poll},
};
use std::{
    env,
    num::NonZeroU64,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

fn options() -> Result<Option<Option<NonZeroU64>>, Error> {
    let mut args = env::args();
    args.next();
    match args.next().as_deref() {
        None => Ok(Some(None)),
        Some("--help") if args.next().is_none() => {
            println!("openpilot-calibrationd [--frames N]\n\nRuns the original calibration estimator and native IPC loop continuously.\nWaits for CarParams, publishes liveCalibration and persists CalibrationParams.\nUses original OPENPILOT_PREFIX/PARAMS_ROOT paths; SIGINT/SIGTERM request shutdown.\n--frames bounds publications for host QA. Production manager selection is unchanged.");
            Ok(None)
        }
        Some("--frames") => {
            let frames = args
                .next()
                .ok_or(Error::Contract("missing frame count"))?
                .parse()
                .map_err(|_| Error::Contract("frame count must be a positive integer"))?;
            if args.next().is_some() {
                return Err(Error::Contract("unexpected argument"));
            }
            Ok(Some(Some(frames)))
        }
        _ => Err(Error::Contract("unknown arguments; see --help")),
    }
}

fn run(frames: Option<NonZeroU64>) -> Result<(), Error> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let limits = platform::configure()?;
    let mut publisher = PubMaster::for_runtime(&["liveCalibration"])?;
    let mut subscriber = SubMaster::for_runtime(
        &["cameraOdometry", "carState"],
        Options {
            poll: Poll::One("cameraOdometry".to_owned()),
            ..Options::default()
        },
    )?;
    let params = parameters::open()?;
    let not_car = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if let Some(bytes) = params.get("CarParams")?.filter(|bytes| !bytes.is_empty()) {
            break wire::not_car(&bytes)?;
        }
        thread::sleep(Duration::from_millis(100));
    };
    let seed = match params
        .get("CalibrationParams")?
        .filter(|bytes| !bytes.is_empty())
    {
        Some(bytes) => {
            let (seed, error) = wire::saved(&bytes);
            if let Some(error) = error {
                eprintln!("Error reading cached CalibrationParams: {error}");
            }
            seed
        }
        None => Seed::default(),
    };
    let mut calibrator = Calibrator::new(limits, seed)?;
    calibrator.not_car = not_car;
    let writer = PendingWrites::new(parameters::open()?)?;
    let debug = env::var_os("DEBUG").is_some();
    let mut remaining = frames.map(NonZeroU64::get);
    while !stop.load(Ordering::Relaxed) {
        let timeout = if subscriber.state.frame() == -1 {
            Duration::ZERO
        } else {
            Duration::from_millis(100)
        };
        subscriber.update(timeout)?;
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let trim = if subscriber.state.topic("cameraOdometry")?.updated {
            parameters::yaw_trim(&params)?
        } else {
            0.0
        };
        let output = loop_state::step(&mut calibrator, &subscriber.state, trim)?;
        if debug {
            if let Some(rpy) = output.update.rpy {
                println!("got new rpy {rpy:?}");
            }
        }
        if output.update.persist {
            writer.put(wire::encode(&calibrator, platform::timestamp()?, true)?)?;
        }
        if output.publish {
            publisher.send(
                "liveCalibration",
                &wire::encode(&calibrator, platform::timestamp()?, output.valid)?,
            )?;
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
    match options().and_then(|value| value.map_or(Ok(()), run)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("calibrationd: {error}");
            ExitCode::FAILURE
        }
    }
}
