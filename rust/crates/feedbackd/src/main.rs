use openpilot_cereal::log_capnp::event;
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::Options,
};
use std::{
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Messaging(#[from] openpilot_messaging::runtime::Error),
    #[error(transparent)]
    State(#[from] openpilot_messaging::state::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error("{0}")]
    Contract(&'static str),
}

fn bookmark() -> Result<Vec<u8>, Error> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let timestamp = u64::try_from(now.tv_sec)
        .ok()
        .and_then(|seconds| seconds.checked_mul(1_000_000_000))
        .and_then(|seconds| {
            u64::try_from(now.tv_nsec)
                .ok()
                .and_then(|nanos| seconds.checked_add(nanos))
        })
        .ok_or(Error::Contract("monotonic timestamp outside u64"))?;
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(timestamp);
    event.set_valid(true);
    event.init_user_bookmark();
    Ok(capnp::serialize::write_message_to_words(&message))
}

fn run() -> Result<(), Error> {
    if let Some(argument) = std::env::args().nth(1) {
        match argument.as_str() {
            "--help" => {
                println!("openpilot-feedbackd\nNative bookmark event forwarding. LKAS audio feedback remains disabled as in the source.");
                return Ok(());
            }
            _ => return Err(Error::Contract("unknown argument")),
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let _params = openpilot_params::Params::for_runtime()?;
    let factory = Factory::for_runtime()?;
    let mut logger = factory.logger();
    let mut publisher = PubMaster::for_runtime(&["userBookmark", "audioFeedback"])?;
    let mut subscriber = SubMaster::for_runtime(
        &["rawAudioData", "bookmarkButton", "carState"],
        Options::default(),
    )?;
    // The source's `if False` prevents all LKAS/audio recording transitions.
    while !stop.load(Ordering::Relaxed) {
        subscriber.update(Duration::from_millis(100))?;
        if subscriber.state.topic("bookmarkButton")?.updated {
            logger.emit(
                log_site!(),
                Record::text(Level::Info, "Bookmark button pressed!".into()),
            )?;
            publisher.send("userBookmark", &bookmark()?)?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("feedbackd: {error}");
            ExitCode::FAILURE
        }
    }
}
