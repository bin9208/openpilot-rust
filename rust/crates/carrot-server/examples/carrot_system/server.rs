use openpilot_carrot_server::{
    config::Config,
    http::{serve, Application},
    params::Backend,
    system::{network::Network, time_sync::TimeSync, Service},
};
use serde::Deserialize;
use std::{
    io::{self, BufRead},
    path::PathBuf,
    sync::Arc,
};

#[derive(Deserialize)]
pub struct Input {
    root: PathBuf,
    source: PathBuf,
    #[serde(default = "available")]
    params: bool,
    #[serde(default)]
    engaged: bool,
    #[serde(default = "clock")]
    now: i64,
}
fn available() -> bool {
    true
}
fn clock() -> i64 {
    1700000000
}

pub async fn run(input: Input) -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("PARAMS_ROOT").as_deref() != Some(input.root.join("params").as_os_str()) {
        return Err("owned PARAMS_ROOT missing before Application construction".into());
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
    let value = Arc::get_mut(&mut app).ok_or("fixture Application shared early")?;
    value.heartbeat_params = None;
    value.auto_update = None;
    value.popular_values = openpilot_carrot_server::popular_values::Service::new(false);
    let now = input.now;
    value.system = Arc::new(Service {
        network: Network::new(input.root.join("bin/nmcli")),
        time_sync: TimeSync {
            localtime: input.root.join("localtime"),
            zoneinfo: input.root.join("zones"),
            now: Arc::new(move || now),
            ..TimeSync::default()
        },
        regulatory: input.root.join("offroad/fcc.html"),
    });
    value.network_refresh = true;
    value.update_live_snapshot(openpilot_carrot_server::Value::object([(
        "services",
        openpilot_carrot_server::Value::object([(
            "selfdriveState",
            openpilot_carrot_server::Value::object([(
                "enabled",
                openpilot_carrot_server::Value::Bool(input.engaged),
            )]),
        )]),
    )]))?;
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
            let line: serde_json::Value = serde_json::from_str(&line).map_err(io::Error::other)?;
            if line["stop"].as_bool() == Some(true) {
                break;
            }
        }
        match stop.send(()) {
            Ok(()) | Err(()) => {}
        }
        Ok(())
    });
    serve(app, listener, async {
        match stopped.await {
            Ok(()) | Err(_) => {}
        }
    })
    .await?;
    reader.await??;
    println!("{{\"stopped\":true}}");
    Ok(())
}
