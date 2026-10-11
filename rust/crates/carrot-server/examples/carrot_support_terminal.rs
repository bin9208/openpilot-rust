use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{support_terminal, terminal, Value};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};
use tokio::{sync::watch, task::JoinSet};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let root = PathBuf::from(input.get("owned_root").string()?);
    if !root.starts_with(std::env::current_dir()?) {
        return Err("fixture root is not owned".into());
    }
    let environment = Value::parse(&std::fs::read_to_string(
        root.join("support-environment.json"),
    )?)?;
    if let Value::Object(fields) = environment {
        for (name, value) in fields {
            std::env::set_var(Value::Text(name).string()?, value.string()?);
        }
    }
    let terminal = terminal::Service::start(terminal::Config {
        launcher: input.get("launcher").string()?.into(),
        cli: root.join("bin/carrot-command"),
        cli_cwd: root.join("repository"),
        start_dir: root.join("repository"),
        motd_dir: root.join("motd"),
        motd_cache: root.join("motd-cache"),
        tmux_log: root.join("tmux.log"),
        web_session: "carrot-terminal".into(),
        capture_lines: 160,
    })?;
    let support = support_terminal::Service::start(support_terminal::Config {
        terminal: Arc::clone(&terminal),
        web: std::env::current_dir()?.join("openpilot/selfdrive/carrot/web"),
        data: root.clone(),
        repository: root.join("repository"),
        launcher: input.get("launcher").string()?.into(),
        params: Some(openpilot_params::Params::for_runtime()?),
        default_ttl: 1800,
        default_command_timeout: 30,
        public_delay: 3.,
        shared_http: true,
    })?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!("{{\"port\":{}}}", listener.local_addr()?.port());
    io::stdout().flush()?;
    let (stop, stopped) = watch::channel(false);
    let mut connections = JoinSet::new();
    let wait = tokio::task::spawn_blocking(|| io::stdin().lock().lines().next());
    tokio::pin!(wait);
    let requested = loop {
        tokio::select! {
            requested = &mut wait => break requested?,
            accepted = listener.accept() => {
                let (socket, _) = accepted?;
                let terminal = Arc::clone(&terminal);
                let support = Arc::clone(&support);
                let mut stopped = stopped.clone();
                connections.spawn(async move {
                    let connection = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(socket), hyper::service::service_fn(move |request| {
                        let terminal = Arc::clone(&terminal);
                        let support = Arc::clone(&support);
                        async move {
                            let request = openpilot_carrot_server::http::decode_request(request);
                            let path = percent_encoding::percent_decode_str(request.uri().path()).decode_utf8_lossy();
                            Ok::<_,std::convert::Infallible>(if support_terminal::http::matches(&path) {
                                support_terminal::http::handle(request, support).await
                            } else { terminal::http::handle(request, terminal).await })
                        }
                    })).with_upgrades();
                    tokio::pin!(connection);
                    tokio::select! { result = &mut connection => result, _ = stopped.changed() => { connection.as_mut().graceful_shutdown(); connection.await } }
                });
            }
            _ = connections.join_next(), if !connections.is_empty() => {},
        }
    };
    drop(listener);
    let mut support_changed = support.changed();
    let mut terminal_changed = terminal.changed();
    support.quiesce();
    terminal.quiesce();
    stop.send_replace(true);
    if requested.transpose()?.is_some_and(|line| line == "force") {
        support.force();
        terminal.force();
    }
    let drain = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        while !connections.is_empty() || !support.is_idle() || !terminal.is_idle() {
            tokio::select! { _ = connections.join_next(), if !connections.is_empty() => {}, _ = support_changed.changed() => {}, _ = terminal_changed.changed() => {} }
        }
    }).await;
    support.force();
    terminal.force();
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    support.shutdown().await?;
    terminal.shutdown().await?;
    println!("{{\"app_cleanup\":true,\"grace_expired\":{},\"support_reaped\":true,\"terminal_reaped\":true}}", drain.is_err());
    io::stdout().flush()?;
    Ok(())
}
