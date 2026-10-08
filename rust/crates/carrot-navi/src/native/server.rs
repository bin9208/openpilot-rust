use super::{
    clock::NativeClock,
    discovery::{self, Beacon},
    http,
    options::Options,
    params::MapReader,
    publisher::Publisher,
    shared::{Close, Shared},
};
use crate::{json::Value, receiver::Receiver, Error};
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use tokio::{
    net::TcpListener,
    signal::unix::{signal, SignalKind},
    time::{Duration, Instant},
};

async fn connections(listener: TcpListener, shared: &Shared) -> Result<(), Error> {
    let mut signal_int = signal(SignalKind::interrupt()).map_err(super::io)?;
    let mut signal_term = signal(SignalKind::terminate()).map_err(super::io)?;
    loop {
        tokio::select! {
            _ = signal_int.recv() => break,
            _ = signal_term.recv() => break,
            accepted = listener.accept() => {
                let (stream, address) = accepted.map_err(super::io)?;
                let owner = shared.clone();
                shared.spawn(async move {
                    let shared = owner.clone();
                    let serve = service_fn(move |request| http::serve(request, shared.clone(), address));
                    let connection = http1::Builder::new().serve_connection(TokioIo::new(stream), serve).with_upgrades();
                    tokio::pin!(connection);
                    let mut closing = owner.closing.subscribe();
                    closing.borrow_and_update();
                    loop {
                        tokio::select! {
                            result = &mut connection => {
                                if let Err(error) = result { eprintln!("HTTP connection: {error}"); }
                                break;
                            }
                            changed = closing.changed() => {
                                if changed.is_err() || *closing.borrow_and_update() == Close::Shutdown {
                                    connection.as_mut().graceful_shutdown();
                                }
                            }
                        }
                    }
                })?;
            }
        }
    }
    shared.closing.send_replace(Close::Shutdown);
    Ok(())
}

async fn serve(shared: &Shared, options: &Options, reader: &MapReader) -> Result<(), Error> {
    for retry in 0..=10 {
        let config = reader.read()?;
        shared.with(|receiver| receiver.set_map_config(config))?;
        match TcpListener::bind((options.host.as_str(), options.port)).await {
            Ok(listener) => {
                tokio::select! {
                    result = connections(listener, shared) => return result,
                    result = watch_map(shared, reader) => return result,
                }
            }
            Err(error) if error.raw_os_error() == Some(98) && retry < 10 => {
                println!(
                    "[carrot_navi] {}:{} still in use; retrying in 0.5s ({}/{})",
                    options.host,
                    options.port,
                    retry + 1,
                    10
                );
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(error) => return Err(super::io(error)),
        }
    }
    Err(Error::value("bind retry exhausted"))
}

async fn watch_map(shared: &Shared, reader: &MapReader) -> Result<(), Error> {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let config = reader.read()?;
        if shared.with(|receiver| receiver.set_map_config(config))? {
            shared.closing.send_replace(Close::MapChanged);
        }
    }
}

async fn cleanup(shared: &Shared) -> Result<(), Error> {
    shared.closing.send_replace(Close::Shutdown);
    let mut tasks = {
        let mut guard = shared
            .tasks
            .lock()
            .map_err(|_| Error::typed("RuntimeError", "task lock poisoned".into()))?;
        std::mem::take(&mut *guard)
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    while !tasks.is_empty() {
        if tokio::time::timeout_at(deadline, tasks.join_next())
            .await
            .is_err()
        {
            tasks.abort_all();
            break;
        }
    }
    while tasks.join_next().await.is_some() {}
    Ok(())
}

pub async fn run(options: Options) -> Result<(), Error> {
    let reader = MapReader::new(options.clone())?;
    let config = reader.read()?;
    let shared = Shared::new(Receiver::new(Value::integer(options.port), config.clone()));
    let mut publisher = if options.cereal {
        match Publisher::start(shared.clone()) {
            Ok(publisher) => Some(publisher),
            Err(error) => {
                let message = Value::text(&format!("publisher unavailable: {error}"));
                shared.with(|receiver| {
                    receiver.record_cereal_publish(Some(&message), &mut NativeClock)
                })??;
                println!("[carrot_navi] cereal publisher unavailable: {error}");
                None
            }
        }
    } else {
        None
    };
    let mut beacon = if options.beacon {
        let beacon = Beacon::start(options.advertised().map(str::to_owned))?;
        let targets = discovery::targets(options.advertised())?;
        let advertised = targets
            .into_iter()
            .map(|(ip, _)| ip)
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "[carrot_navi] discovery advertising {}:{} via UDP 7705",
            if advertised.is_empty() {
                "unavailable"
            } else {
                &advertised
            },
            options.port
        );
        Some(beacon)
    } else {
        None
    };
    println!("[carrot_navi] starting receiver on {}:{} map_theme={} map_type={} map_hz={} map_bitrate_kbps={} screen_center_y_ratio={:.2}", options.host, options.port, config.theme, config.map_type, config.hz, config.bitrate_kbps, config.screen_center_y_ratio);
    let result = serve(&shared, &options, &reader).await;
    let cleanup = cleanup(&shared).await;
    if let Some(beacon) = &mut beacon {
        beacon.stop();
    }
    if let Some(publisher) = &mut publisher {
        publisher.stop();
    }
    result.and(cleanup)
}
