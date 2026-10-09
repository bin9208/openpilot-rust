mod application;
mod http;

use crate::{
    network::Network,
    session::{Publishers, Session},
    Error,
};
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use openpilot_params::Params;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{watch, Mutex},
    task::JoinSet,
};

struct SessionHandle {
    identifier: String,
    client_key: String,
    road: bool,
    value: Mutex<Session>,
}

type Sessions = Vec<Rc<SessionHandle>>;
struct Application {
    carrot: bool,
    network: Network,
    streams: RefCell<Sessions>,
    stream_lock: Mutex<()>,
    publishers: RefCell<Publishers>,
    params: Option<Params>,
    active: Cell<Option<bool>>,
    shutting_down: Cell<bool>,
}

async fn serve_connection(
    stream: tokio::net::TcpStream,
    remote: std::net::SocketAddr,
    app: Rc<Application>,
    mut shutdown: watch::Receiver<bool>,
) {
    let handler = service_fn(move |request| {
        let app = Rc::clone(&app);
        async move { app.handle(request, remote).await }
    });
    let mut connection =
        std::pin::pin!(hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), handler));
    let result = tokio::select! {
        result = &mut connection => result,
        result = shutdown.changed() => { if let Err(error) = result { eprintln!("WebRTC shutdown notification failed: {error}"); } connection.as_mut().graceful_shutdown(); connection.await },
    };
    if let Err(error) = result {
        eprintln!("WebRTC HTTP connection failed: {error}");
    }
}

/// Runs the native source-owned HTTP, peer and IPC event loops.
///
/// # Errors
/// Returns startup, listener or signal initialization failures.
pub async fn serve(host: &str, port: u16, carrot: bool, network: Network) -> Result<(), Error> {
    let app = Rc::new(Application::new(carrot, network)?);
    let listener = TcpListener::bind((host, port)).await?;
    println!("webrtcd listening {}", listener.local_addr()?);
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let (shutdown, notification) = watch::channel(false);
    let mut background = JoinSet::new();
    let maintenance = Rc::clone(&app);
    background.spawn_local(async move { maintenance.maintain().await });
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, remote) = accepted?;
                connections.spawn_local(serve_connection(stream, remote, Rc::clone(&app), notification.clone()));
            },
            signal = tokio::signal::ctrl_c() => { signal?; break; },
            _ = terminate.recv() => break,
            joined = connections.join_next(), if !connections.is_empty() => { if let Some(Err(error)) = joined { eprintln!("WebRTC HTTP worker failed: {error}"); } },
        }
    }
    drop(listener);
    if let Err(error) = shutdown.send(true) {
        eprintln!("WebRTC HTTP shutdown notification failed: {error}");
    }
    background.abort_all();
    while let Some(result) = background.join_next().await {
        if let Err(error) = result {
            if !error.is_cancelled() {
                eprintln!("WebRTC maintenance failed: {error}");
            }
        }
    }
    app.shutdown().await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while !connections.is_empty() {
        match tokio::time::timeout_at(deadline, connections.join_next()).await {
            Ok(Some(Err(error))) => eprintln!("WebRTC HTTP worker failed during shutdown: {error}"),
            Ok(Some(Ok(())) | None) => {}
            Err(_) => {
                connections.abort_all();
                break;
            }
        }
    }
    Ok(())
}

fn affinity() -> Result<(), Error> {
    let configured =
        std::env::var("CARROT_VISION_WEBRTC_CORES").unwrap_or_else(|_| "0,1,2,3".to_owned());
    let mut set = nix::sched::CpuSet::new();
    let mut count = 0;
    for core in configured.split(',').filter(|core| !core.trim().is_empty()) {
        set.set(
            core.trim()
                .parse::<usize>()
                .map_err(|_| Error::Contract("invalid Carrot Vision CPU affinity"))?,
        )?;
        count += 1;
    }
    if count > 0 {
        nix::sched::sched_setaffinity(nix::unistd::Pid::from_raw(0), &set)?;
    }
    Ok(())
}

/// Executes the standalone original daemon CLI and native local event loop.
///
/// # Errors
/// Returns argument, runtime or server startup failures.
pub fn entrypoint(carrot: bool) -> Result<(), Error> {
    let mut host = "0.0.0.0".to_owned();
    let mut port = 5001;
    let mut debug = false;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--host" => host = args.next().ok_or(Error::Contract("missing host"))?,
            "--port" => {
                port = args
                    .next()
                    .ok_or(Error::Contract("missing port"))?
                    .parse()
                    .map_err(|_| Error::Contract("invalid port"))?;
            }
            "--debug" => debug = true,
            "--help" | "-h" => {
                println!("Usage: webrtcd [--host HOST] [--port PORT] [--debug]");
                return Ok(());
            }
            _ => return Err(Error::Contract("unknown argument")),
        }
    }
    if debug {
        return Err(Error::Contract("generated debug video remains unported"));
    }
    if carrot {
        if let Err(error) = affinity() {
            eprintln!("WebRTC Carrot Vision affinity failed: {error}");
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    tokio::task::LocalSet::new()
        .block_on(&runtime, serve(&host, port, carrot, Network::for_runtime()))
}
