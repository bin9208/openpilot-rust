use openpilot_carrot_server::{
    config::Config, http::Application, params::Backend, support_terminal, terminal, Value,
};
use std::{path::PathBuf, sync::Arc, time::Duration};

async fn fail_owner(
    config: support_terminal::Config,
) -> Result<Arc<support_terminal::Service>, Box<dyn std::error::Error>> {
    let old = rustix::process::getrlimit(rustix::process::Resource::Nofile);
    rustix::process::setrlimit(
        rustix::process::Resource::Nofile,
        rustix::process::Rlimit {
            current: Some(0),
            maximum: old.maximum,
        },
    )?;
    let service = support_terminal::Service::start(config);
    let unavailable = match &service {
        Ok(service) => service.snapshot().await.is_err(),
        Err(_) => false,
    };
    rustix::process::setrlimit(rustix::process::Resource::Nofile, old)?;
    if !unavailable {
        return Err("owned support reactor did not fail at exhausted descriptor limit".into());
    }
    Ok(service?)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("owned root required")?);
    let cwd = std::env::current_dir()?;
    if !root.starts_with(&cwd) || root == cwd {
        return Err("fixture root is not owned".into());
    }
    std::fs::create_dir_all(root.join("web"))?;
    let mut config = Config::at(&cwd, &root, &root.join("settings.json"));
    config.web = root.join("web");
    config.legacy_state = root.join("legacy");
    let mut app = Application::new(config, Backend::memory(root.join("state")));
    let application = Arc::get_mut(&mut app).ok_or("fixture Application shared")?;
    application.terminal = Some(terminal::Service::start(terminal::Config {
        launcher: root.join("launcher"),
        cli: root.join("command"),
        cli_cwd: root.clone(),
        start_dir: root.clone(),
        motd_dir: root.join("motd"),
        motd_cache: root.join("motd-cache"),
        tmux_log: root.join("tmux.log"),
        web_session: "owned-cleanup".into(),
        capture_lines: 160,
    })?);
    let terminal = application.terminal.as_ref().ok_or("terminal absent")?;
    terminal.snapshot().await?;
    let live = application.live.as_ref().ok_or("live absent")?;
    live.snapshot(false).await?;
    let navi = application.web_navi.as_ref().ok_or("navi absent")?;
    navi.status().await?;
    if let Some(healthy) = &application.support_terminal {
        healthy.snapshot().await?;
        healthy.shutdown().await?;
    }
    let support = support_terminal::Config::original(application, Arc::clone(terminal))?;
    application.support_terminal = Some(fail_owner(support).await?);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let retained = Arc::clone(&app);
    let result = openpilot_carrot_server::http::serve(app, listener, async {
        tokio::time::sleep(Duration::from_millis(50)).await;
    })
    .await;
    let error = result.err().ok_or("cleanup failure was not surfaced")?;
    let terminal_stopped = retained
        .terminal
        .as_ref()
        .ok_or("terminal absent")?
        .snapshot()
        .await
        .is_err();
    let live_stopped = retained
        .live
        .as_ref()
        .ok_or("live absent")?
        .snapshot(false)
        .await
        .is_err();
    let navi_stopped = retained
        .web_navi
        .as_ref()
        .ok_or("navi absent")?
        .status()
        .await
        .is_err();
    println!(
        "{}",
        Value::object([
            ("cleanup_error", Value::text(&error.to_string())),
            ("terminal_stopped", Value::Bool(terminal_stopped)),
            ("live_stopped", Value::Bool(live_stopped)),
            ("navi_stopped", Value::Bool(navi_stopped)),
            ("retained_application", Value::Bool(true)),
        ])
        .encode()?
    );
    if !(terminal_stopped && live_stopped && navi_stopped) {
        return Err("a later Application owner survived cleanup failure".into());
    }
    Ok(())
}
