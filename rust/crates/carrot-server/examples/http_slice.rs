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
    let params = openpilot_params::Params::for_runtime_at(&path("params")?)?;
    let backend = Backend::native(params, config.state.clone());
    let app = Application::new(config, backend);
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
