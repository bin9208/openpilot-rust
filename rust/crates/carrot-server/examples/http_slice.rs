use openpilot_carrot_server::{
    config::Config,
    http::{serve, Application},
    params::Backend,
    Value,
};
use std::{
    io::{self, BufRead},
    path::PathBuf,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let path = |name| -> Result<PathBuf, openpilot_carrot_navi::Error> {
        Ok(input.get(name).string()?.into())
    };
    let mut config = Config::at(&path("repository")?, &path("data")?, &path("settings")?);
    config.web = path("web")?;
    config.shared_assets = path("shared_assets")?;
    config.training_assets = path("training_assets")?;
    config.legacy_state = path("legacy_state")?;
    if input.has("params_backup") {
        config.params_backup = path("params_backup")?;
    }
    let backend = if input.get("unavailable").truth() {
        Backend::memory(config.state.clone())
    } else {
        Backend::native(
            openpilot_params::Params::for_runtime_at(&path("params")?)?,
            config.state.clone(),
        )
    };
    let mut app = Application::new(config, backend);
    if input.has("cars") {
        std::sync::Arc::get_mut(&mut app)
            .ok_or("fixture Application is already shared")?
            .cars = openpilot_carrot_server::cars::Cars::at(path("cars")?);
    }
    if input.has("timestamp") {
        let application =
            std::sync::Arc::get_mut(&mut app).ok_or("fixture Application is already shared")?;
        application.history = openpilot_carrot_server::param_changes::History::new(
            openpilot_carrot_server::param_changes::Paths {
                log: application.config.state.join("param_changes.jsonl"),
                baseline: application.config.state.join("fingerprint_baseline.json"),
            },
        )
        .with_timestamp(input.get("timestamp").clone());
        application.intro = openpilot_carrot_server::intro::Intro::with_timestamp(
            application.config.clone(),
            1700000000,
        );
    }
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut command = String::new();
        let _ = io::stdin().lock().read_line(&mut command);
        let _ = stop.send(());
    });
    serve(app, listener, async {
        let _ = stopped.await;
    })
    .await?;
    input_worker
        .join()
        .map_err(|_| "fixture input thread panicked")?;
    Ok(())
}
