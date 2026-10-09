use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    terminal::{Config, Service},
    Value,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};
use tokio::{sync::watch, task::JoinSet};
#[path = "carrot_terminal/application.rs"]
mod application;
#[path = "carrot_terminal/config.rs"]
mod command_config;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let root = PathBuf::from(input.get("owned_root").string()?);
    if !root.starts_with(std::env::current_dir()?) {
        return Err("fixture root is not owned".into());
    }
    let config = Config {
        launcher: PathBuf::from(input.get("launcher").string()?),
        cli: root.join("bin/carrot-command"),
        cli_cwd: root.join("repository"),
        start_dir: root.join("repository"),
        motd_dir: root.join("motd"),
        motd_cache: root.join("motd-cache"),
        tmux_log: root.join("tmux.log"),
        web_session: "carrot-terminal".into(),
        capture_lines: 160,
    };
    let terminal = Service::start(config)?;
    let vision = if input.get("cli_config").truth() {
        Some(
            command_config::configuration(std::path::Path::new(
                &input.get("cli_config").string()?,
            ))?
            .1,
        )
    } else {
        None
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!("{{\"port\":{}}}", listener.local_addr()?.port());
    io::stdout().flush()?;
    if input.get("application").truth() {
        return application::run(input, root, terminal, listener).await;
    }
    let (stop, stopped) = watch::channel(false);
    let mut connections = JoinSet::new();
    let wait = tokio::task::spawn_blocking(|| io::stdin().lock().lines().next());
    tokio::pin!(wait);
    loop {
        tokio::select! {
            _ = &mut wait => break,
            accepted = listener.accept() => {
                let (socket, _) = accepted?;
                let service = Arc::clone(&terminal);
                let vision = vision.clone();
                let mut stopped = stopped.clone();
                connections.spawn(async move {
                    let connection = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(socket), hyper::service::service_fn(move |request| {
                        let service = Arc::clone(&service);
                        let vision = vision.clone();
                        async move {
                            let request = openpilot_carrot_server::http::decode_request(request);
                            Ok::<_,std::convert::Infallible>(if request.uri().path() == "/api/vision_test/status" { openpilot_carrot_server::vision_test::http::handle(request, vision).await } else { openpilot_carrot_server::terminal::http::handle(request, service).await })
                        }
                    })).with_upgrades();
                    tokio::pin!(connection);
                    tokio::select! { result = &mut connection => result, _ = stopped.changed() => { connection.as_mut().graceful_shutdown(); connection.await } }
                });
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    drop(listener);
    terminal.quiesce();
    stop.send_replace(true);
    let mut changed = terminal.changed();
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        while !connections.is_empty() || !terminal.is_idle() {
            tokio::select! { _ = connections.join_next(), if !connections.is_empty() => {}, _ = changed.changed() => {} }
        }
    }).await?;
    println!(
        "{{\"app_cleanup\":true,\"pty\":{}}}",
        terminal.snapshot().await?.encode()?
    );
    io::stdout().flush()?;
    tokio::task::spawn_blocking(|| io::stdin().lock().lines().next()).await?;
    terminal.shutdown().await?;
    Ok(())
}
