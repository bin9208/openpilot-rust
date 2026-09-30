use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::{Options, Poll},
};
use openpilot_params::Params;
use openpilot_torqued::{
    estimator::Estimator,
    loop_state::{self, TOPICS},
    numerics::Numerics,
    parameters::PendingWrites,
    platform,
    random::RandomState,
    wire, Error,
};
use std::{
    env,
    num::NonZeroU64,
    path::PathBuf,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
struct Arguments {
    numerics: PathBuf,
    frames: Option<NonZeroU64>,
}
fn arguments() -> Result<Option<Arguments>, Error> {
    let mut result = Arguments {
        numerics: env::var_os("TORQUED_NUMERICS")
            .map(PathBuf::from)
            .unwrap_or(env::current_exe()?.with_file_name("torqued-numerics")),
        frames: None,
    };
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" => {
                println!("openpilot-torqued [--numerics DIRECTORY] [--frames N] [--demo]\nContinuous torque estimation using original IPC and Params.\nRequires pinned native OpenBLAS artifact (TORQUED_NUMERICS or sibling torqued-numerics).\n--frames bounds publications for host QA; --demo retains original no-op behavior.\nSIGINT/SIGTERM drain pending Params writes. Production manager selection is unchanged.");
                return Ok(None);
            }
            "--numerics" => {
                result.numerics = args
                    .next()
                    .ok_or(Error::Contract("missing numerical directory"))?
                    .into()
            }
            "--frames" => {
                result.frames = Some(
                    args.next()
                        .ok_or(Error::Contract("missing frame count"))?
                        .parse()
                        .map_err(|_| Error::Contract("frame count must be positive"))?,
                )
            }
            "--demo" => (),
            _ => return Err(Error::Contract("unknown argument; see --help")),
        }
    }
    Ok(Some(result))
}
fn run(args: Arguments) -> Result<(), Error> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    platform::configure()?;
    let debug = env::var("DEBUG")
        .unwrap_or_else(|_| "0".to_owned())
        .trim()
        .parse::<i128>()
        .map_err(|_| Error::Contract("DEBUG must be an integer"))?
        != 0;
    let mut fit = Numerics::load(&args.numerics)?;
    let mut publisher = PubMaster::for_runtime(&["liveTorqueParameters"])?;
    let mut subscriber = SubMaster::for_runtime(
        &TOPICS,
        Options {
            poll: Poll::One("livePose".to_owned()),
            ..Options::default()
        },
    )?;
    let params = Params::for_runtime()?;
    let car = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if let Some(bytes) = params.get("CarParams")?.filter(|bytes| !bytes.is_empty()) {
            break wire::car(&bytes)?;
        }
        thread::sleep(Duration::from_millis(100));
    };
    let mut estimator = Estimator::new(car, (false, false), RandomState::entropy()?);
    if let Some(error) = wire::restore(
        &mut estimator,
        params.get("CarParamsPrevRoute")?.as_deref(),
        params.get("LiveTorqueParameters")?.as_deref(),
    )? {
        eprintln!("torqued: failed to restore cached torque parameters: {error}");
        match params.remove("LiveTorqueParameters") {
            Ok(()) => (),
            Err(openpilot_params::Error::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let writer = PendingWrites::new(Params::for_runtime()?)?;
    let mut remaining = args.frames.map(NonZeroU64::get);
    while !stop.load(Ordering::Relaxed) {
        subscriber.update(Duration::from_millis(100))?;
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let actions = loop_state::step(&mut estimator, &subscriber.state)?;
        if actions.publish {
            publisher.send(
                "liveTorqueParameters",
                &wire::encode(
                    &estimator.message(&mut fit, debug)?,
                    platform::timestamp()?,
                    actions.valid,
                )?,
            )?;
            if let Some(count) = &mut remaining {
                *count -= 1;
            }
        }
        if actions.persist {
            writer.put(wire::encode(
                &estimator.message(&mut fit, true)?,
                platform::timestamp()?,
                actions.valid,
            )?)?;
        }
        if remaining == Some(0) {
            break;
        }
    }
    Ok(())
}
fn main() -> ExitCode {
    match arguments().and_then(|args| args.map_or(Ok(()), run)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("torqued: {error}");
            ExitCode::FAILURE
        }
    }
}
