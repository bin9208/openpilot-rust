use super::Shutdown;
use crate::{config::Config, http::Application, params::Backend, web_sound_http::Sessions, Error};
use futures_util::StreamExt;
use hyper_util::rt::TokioIo;
use std::{
    convert::Infallible,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
};
use tokio_tungstenite::tungstenite::{error::ProtocolError, Error as WsError, Message};

fn error(error: impl std::fmt::Display) -> Error {
    Error::Source(error.to_string())
}

fn setup(ipc: PathBuf) -> Result<(tempfile::TempDir, Arc<Application>), Error> {
    std::fs::create_dir(ipc)?;
    let owned = tempfile::tempdir()?;
    let config = Config::at(
        owned.path(),
        &owned.path().join("data"),
        &owned.path().join("settings"),
    );
    let params = openpilot_params::Params::for_runtime_at(&owned.path().join("params"))?;
    let app = Application::new(config.clone(), Backend::native(params, config.state));
    Ok((owned, app))
}

fn cleanup(ipc: PathBuf, owned: tempfile::TempDir) -> std::io::Result<()> {
    std::fs::remove_dir_all(ipc)?;
    owned.close()
}

fn isolated_force() -> Result<(), Error> {
    let prefix = format!("websound_{}", uuid::Uuid::new_v4().simple());
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "web_sound::force_tests::explicit_force_wakes_receiver_and_reaps_active_sender",
            "--nocapture",
        ])
        .env("OPENPILOT_PREFIX", &prefix)
        .env("CARROT_WEB_SOUND_FORCE_CHILD", &prefix)
        .spawn()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                assert!(status.success(), "isolated Force test exited {status}");
                return Ok(());
            }
            Ok(None) => {}
            Err(error) => {
                child.kill()?;
                child.wait()?;
                return Err(error.into());
            }
        }
        if started.elapsed() >= Duration::from_secs(10) {
            child.kill()?;
            child.wait()?;
            return Err(Error::Source("isolated Force test timed out".into()));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_force_wakes_receiver_and_reaps_active_sender() -> Result<(), Error> {
    let prefix = std::env::var("OPENPILOT_PREFIX").unwrap_or_default();
    if !prefix.starts_with("websound_")
        || std::env::var("CARROT_WEB_SOUND_FORCE_CHILD")
            .ok()
            .as_deref()
            != Some(prefix.as_str())
    {
        return tokio::task::spawn_blocking(isolated_force)
            .await
            .map_err(error)?;
    }
    tokio::task::LocalSet::new().run_until(async {
        let ipc = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
        let ipc_setup = ipc.clone();
        let (owned, app) = tokio::task::spawn_blocking(move || setup(ipc_setup)).await.map_err(error)??;
        let (shutdown, receiver) = watch::channel(Shutdown::Running);
        let (sessions, mut launches) = Sessions::channel(receiver);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let mut connections = JoinSet::new();
        connections.spawn(async move {
            let (socket, _) = listener.accept().await?;
            let service = hyper::service::service_fn(move |request| {
                let response = crate::web_sound_http::handle(request, &app, &sessions);
                async move { Ok::<_, Infallible>(response) }
            });
            hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(socket), service).with_upgrades().await.map_err(error)?;
            Ok::<(), Error>(())
        });
        let socket = TcpStream::connect(address).await?;
        let (mut client, response) = tokio_tungstenite::client_async(
            format!("ws://{address}/ws/web_sound"), socket,
        ).await.map_err(error)?;
        assert_eq!(response.status(), hyper::StatusCode::SWITCHING_PROTOCOLS);
        let launch = launches.recv().await.ok_or_else(|| Error::Source("force fixture launch absent".into()))?;
        let mut sounds = JoinSet::new();
        sounds.spawn_local(crate::web_sound_http::run(launch));
        let initial = tokio::time::timeout(Duration::from_secs(2), client.next()).await.map_err(error)?;
        assert!(matches!(&initial, Some(Ok(Message::Text(_)))));
        shutdown.send(Shutdown::Force).map_err(error)?;
        let closed = tokio::time::timeout(Duration::from_secs(1), client.next()).await.map_err(error)?;
        assert!(matches!(closed, Some(Err(WsError::Protocol(ProtocolError::ResetWithoutClosingHandshake)))));
        let joined = tokio::time::timeout(Duration::from_secs(1), sounds.join_next()).await.map_err(error)?;
        joined.ok_or_else(|| Error::Source("force fixture session absent".into()))?.map_err(error)??;
        while let Some(joined) = connections.join_next().await {
            joined.map_err(error)??;
        }
        println!("explicit Force: upgrade101, initial soundState={initial:?}, peerEOF={closed:?}, session returned Ok after sender cancellation/reap");
        tokio::task::spawn_blocking(move || cleanup(ipc, owned)).await.map_err(error)??;
        Ok(())
    }).await
}
