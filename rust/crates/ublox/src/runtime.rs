use crate::{
    native::{self, Config, Native},
    parser::Parser,
    pigeon::{Pigeon, Platform},
    Error,
};
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::runtime::PubMaster;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub fn receiver(config: Config, stop: Arc<AtomicBool>) -> Result<(), Error> {
    if !config.root.join("TICI").is_file() {
        return Err(Error::Malformed("unsupported hardware for pigeond"));
    }
    if config.root != std::path::Path::new("/")
        && !std::env::var("OPENPILOT_PREFIX")
            .unwrap_or_default()
            .starts_with("rust-probe-")
    {
        return Err(Error::Malformed(
            "alternate root requires isolated rust-probe IPC",
        ));
    }
    let mut publisher = PubMaster::for_runtime(&["ubloxRaw"])?;
    let mut pigeon = Pigeon {
        platform: Native::open(config, stop)?,
    };
    let result = receive_loop(&mut pigeon, &mut publisher);
    match result {
        Err(Error::Interrupted) => pigeon.deinitialize(),
        result => result,
    }
}
fn receive_loop(pigeon: &mut Pigeon<Native>, publisher: &mut PubMaster) -> Result<(), Error> {
    pigeon.init()?;
    let mut last_save = pigeon.platform.monotonic();
    loop {
        let bytes = pigeon.platform.receive()?;
        if bytes.first() == Some(&0) {
            pigeon
                .platform
                .log("warning", "received invalid data from ublox, re-initing!")?;
            pigeon.init()?;
            continue;
        }
        if bytes.is_empty() {
            pigeon.platform.sleep(0.001)?;
            continue;
        }
        let mut message = capnp::message::Builder::new_default();
        let mut root = message.init_root::<event::Builder<'_>>();
        root.set_valid(true);
        root.set_log_mono_time((native::monotonic() * 1e9) as u64);
        root.set_ublox_raw(&bytes);
        publisher.send(
            "ubloxRaw",
            &capnp::serialize::write_message_to_words(&message),
        )?;
        if pigeon.platform.monotonic() - last_save > 300. {
            pigeon.save_almanac()?;
            last_save = pigeon.platform.monotonic();
        }
    }
}
pub fn decoder(stop: &AtomicBool) -> Result<(), Error> {
    let mut parser = Parser::default();
    let mut publisher = PubMaster::for_runtime(&["ubloxGnss", "gpsLocationExternal"])?;
    let capacity = openpilot_messaging::services::lookup("ubloxRaw")
        .ok_or(Error::Malformed("ubloxRaw service"))?
        .queue_size;
    let mut subscriber = openpilot_msgq::Subscriber::for_runtime("ubloxRaw", false, capacity)?;
    while !stop.load(Ordering::Relaxed) {
        let Some(bytes) = subscriber.receive(Duration::from_millis(100))? else {
            continue;
        };
        let message =
            capnp::serialize::read_message(bytes.as_slice(), capnp::message::ReaderOptions::new())?;
        let root = message.get_root::<event::Reader<'_>>()?;
        let time = root.get_log_mono_time() as f64 * 1e-9;
        let data = match root.which().map_err(|_| Error::Malformed("event union"))? {
            event::Which::UbloxRaw(bytes) => bytes?,
            _ => return Err(Error::Malformed("expected ubloxRaw")),
        };
        for frame in parser.framer.add_data(time, data) {
            if let Ok(Some(packet)) = parser.parse_frame(&frame, (native::monotonic() * 1e9) as u64)
            {
                publisher.send(packet.service, &packet.bytes)?;
            }
        }
    }
    Ok(())
}
