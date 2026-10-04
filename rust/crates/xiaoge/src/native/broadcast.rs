use super::{
    options::Options,
    platform::{self, Stop},
    tcp,
    tesla::Tesla,
    vision, Error,
};
use crate::wire;
use openpilot_cereal::log_capnp::event;
use openpilot_logging::{Fields, Value};
use openpilot_messaging::{runtime::SubMaster, state::Options as SubscriptionOptions};
use openpilot_params::Params;
use std::{
    net::UdpSocket,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};

fn device_ip() -> Result<String, std::io::Error> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    Ok(socket.local_addr()?.ip().to_string())
}

pub fn run(options: Options) -> Result<(), Error> {
    let stop = Stop::new()?;
    let interrupted = Arc::clone(&stop.requested);
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("xiaoge-main".to_owned())
        .spawn(move || {
            let result = owner(options, &interrupted);
            if sender.send(result).is_err() && !interrupted.load(Ordering::Acquire) {
                eprintln!("Xiaoge main result receiver closed");
            }
        })?;
    loop {
        if stop.requested() {
            return Ok(());
        }
        match receiver.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::Contract("Xiaoge owner stopped without result"))
            }
        }
    }
}

fn owner(options: Options, interrupted: &Arc<AtomicBool>) -> Result<(), Error> {
    let ip = match &options.device_ip {
        Some(ip) => ip.clone(),
        None => device_ip().unwrap_or_else(|_| "127.0.0.1".to_owned()),
    };
    let params = Params::for_runtime()?;
    let mut subscriber = SubMaster::for_runtime(
        &["carState", "modelV2", "selfdriveState"],
        SubscriptionOptions::default(),
    )?;
    let running = Arc::new(AtomicBool::new(true));
    let clients = Arc::new(Mutex::new(Vec::new()));
    let _tcp = tcp::start(options.tcp, Arc::clone(&clients), Arc::clone(&running))?;
    let root = options.root.clone();
    let assets = options.assets.clone();
    let config = options.config.clone();
    let http = options.http;
    let vision_running = Arc::clone(&running);
    let _vision = thread::Builder::new()
        .name("xiaoge-vision".to_owned())
        .spawn(move || {
            if let Err(error) = vision::run(
                vision::Options {
                    root,
                    assets,
                    config,
                    address: http,
                },
                vision_running,
            ) {
                eprintln!("Xiaoge vision server error: {error}");
            }
        })?;
    let mut tesla = Tesla::default();
    let mut sequence = 0u64;
    let mut next = None;
    let result = (|| {
        while running.load(Ordering::Acquire) && !interrupted.load(Ordering::Acquire) {
            subscriber.update(Duration::ZERO)?;
            let mut data = Fields::new();
            let car = subscriber.state.topic("carState")?;
            if car.alive {
                let event::CarState(reader) = car.event()?.which().map_err(capnp::Error::from)?
                else {
                    return Err(Error::Contract("unexpected carState event"));
                };
                let mut value = wire::car(reader?);
                if let Some(tesla) = tesla.collect(&params, &options.root)? {
                    value.extend(
                        tesla
                            .into_iter()
                            .map(|(name, value)| (name.to_owned(), value)),
                    );
                }
                data.insert("carState".to_owned(), Value::Object(value));
            }
            let model = subscriber.state.topic("modelV2")?;
            if model.alive {
                let event::ModelV2(reader) = model.event()?.which().map_err(capnp::Error::from)?
                else {
                    return Err(Error::Contract("unexpected modelV2 event"));
                };
                data.insert("modelV2".to_owned(), Value::Object(wire::model(reader?)?));
            }
            let system = subscriber.state.topic("selfdriveState")?;
            if system.alive {
                let event::SelfdriveState(reader) =
                    system.event()?.which().map_err(capnp::Error::from)?
                else {
                    return Err(Error::Contract("unexpected selfdriveState event"));
                };
                data.insert(
                    "systemState".to_owned(),
                    Value::Object(wire::system(reader?)),
                );
            }
            tcp::broadcast(
                &clients,
                &wire::packet(sequence, platform::monotonic()?, &ip, data)?,
            )?;
            sequence = sequence
                .checked_add(1)
                .ok_or(Error::Contract("TCP sequence overflow"))?;
            let deadline = match next {
                Some(value) => value,
                None => platform::monotonic()? + 0.05,
            };
            next = Some(deadline + 0.05);
            let remaining = deadline - platform::monotonic()?;
            if remaining > 0.0 {
                platform::sleep(remaining)?;
            }
            if options
                .frames
                .is_some_and(|frames| sequence >= frames.get())
            {
                break;
            }
        }
        Ok::<_, Error>(())
    })();
    if let Err(error) = result {
        eprintln!("XiaogeDataBroadcaster error: {error}");
    }
    running.store(false, Ordering::Release);
    tcp::shutdown(&clients)?;
    Ok(())
}
