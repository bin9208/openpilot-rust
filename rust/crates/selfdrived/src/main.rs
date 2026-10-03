use openpilot_selfdrived::{
    callbacks,
    controller::Error,
    runtime::{self, Runtime},
};
use std::{
    env,
    num::NonZeroU64,
    path::PathBuf,
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
};

fn options() -> Result<Option<Option<NonZeroU64>>, Error> {
    let mut args = env::args();
    args.next();
    match args.next().as_deref() {
        None => Ok(Some(None)),
        Some("--help") if args.next().is_none() => {
            println!("openpilot-selfdrived [--frames N]\n\nRuns selfdrive state, events and alerts continuously using native cereal/msgq/Params.\ncarState is nonconflated with a 20ms receive timeout. SIGINT/SIGTERM request shutdown.\n--frames bounds iterations for host QA. Production manager selection is unchanged.");
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
    runtime::platform::configure()?;
    let root = env::var_os("OPENPILOT_ROOT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        PathBuf::from,
    );
    if let Some(mut runtime) = Runtime::open(&root, &stop)? {
        runtime.run(stop, frames)?;
    }
    Ok(())
}
fn main() -> ExitCode {
    match options().and_then(|value| value.map_or(Ok(()), run)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Parameter(callbacks::Error::IntegerParameter { .. })) => std::process::abort(),
        Err(error) => {
            eprintln!("selfdrived: {error}");
            ExitCode::FAILURE
        }
    }
}
