use openpilot_carrot_server::{
    config::Config,
    http::{serve, Application},
    param_changes::{History, Paths},
    params::Backend,
    popular_values::Service,
    Value,
};
use serde::Deserialize;
use std::{
    io::{self, BufRead},
    path::PathBuf,
    sync::Arc,
};

#[derive(Deserialize)]
struct Input {
    root: PathBuf,
    source: PathBuf,
    params: bool,
    popular: Option<serde_json::Value>,
    #[serde(default)]
    refresh: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut first = String::new();
    io::stdin().read_line(&mut first)?;
    let input: Input = serde_json::from_str(&first)?;
    if std::env::var_os("PARAMS_ROOT").as_deref() != Some(input.root.join("params").as_os_str()) {
        return Err("owned PARAMS_ROOT required before Application construction".into());
    }
    let config = Config::at(
        &input.source,
        &input.root,
        &input.root.join("settings.json"),
    );
    let backend = if input.params {
        Backend::native(
            openpilot_params::Params::open(&input.root.join("params"), "d")?,
            config.state.clone(),
        )
    } else {
        Backend::memory(config.state.clone())
    };
    let mut app = Application::new(config, backend);
    let value = Arc::get_mut(&mut app).ok_or("Application shared before fixture setup")?;
    value.heartbeat_params = None;
    value.auto_update = None;
    value.network_refresh = false;
    value.popular_values = Service::new(input.refresh);
    value.popular_values.seed(
        input
            .popular
            .map(|raw| Value::parse(&raw.to_string()))
            .transpose()?,
    )?;
    value.history = History::new(Paths {
        log: input.root.join("state/param_changes.jsonl"),
        baseline: input.root.join("state/fingerprint_baseline.json"),
    })
    .with_timestamp(Value::integer(1700000000));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!(
        "{{\"ready\":true,\"port\":{}}}",
        listener.local_addr()?.port()
    );
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let reader = tokio::task::spawn_blocking(move || -> io::Result<()> {
        for line in io::stdin().lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                break;
            }
            let value: serde_json::Value = serde_json::from_str(&line).map_err(io::Error::other)?;
            if value["stop"].as_bool() == Some(true) {
                break;
            }
        }
        match stop.send(()) {
            Ok(()) | Err(()) => {}
        }
        Ok(())
    });
    serve(app, listener, async move {
        match stopped.await {
            Ok(()) | Err(_) => {}
        }
    })
    .await?;
    reader.await??;
    println!("{{\"stopped\":true}}");
    Ok(())
}
