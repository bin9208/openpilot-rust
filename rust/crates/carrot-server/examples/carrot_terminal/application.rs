use openpilot_carrot_server::{
    config::Config, http::Application, params::Backend, terminal::Service, Value,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

pub async fn run(
    input: Value,
    root: PathBuf,
    terminal: Arc<Service>,
    listener: tokio::net::TcpListener,
) -> Result<(), Box<dyn std::error::Error>> {
    let cli = PathBuf::from(input.get("cli_config").string()?);
    let (_, vision) = super::command_config::configuration(&cli)?;
    let mut config = Config::at(
        &std::env::current_dir()?,
        &root,
        &root.join("settings.json"),
    );
    config.web = root.join("web");
    config.legacy_state = root.join("legacy-state");
    let mut app = Application::new(config, Backend::memory(root.join("state")));
    let writable = Arc::get_mut(&mut app).ok_or("fixture Application already shared")?;
    writable.terminal = Some(Arc::clone(&terminal));
    writable.vision_test = Some(vision);
    if root.join("stream-target.json").is_file() {
        let target = Value::parse(&std::fs::read_to_string(root.join("stream-target.json"))?)?;
        writable.stream_proxy = Some(Arc::new(
            openpilot_carrot_server::stream_proxy::Service::new(
                target.get("target").string()?,
                Some(openpilot_params::Params::for_runtime()?),
            )?,
        ));
    }
    if root.join("support-environment.json").is_file() {
        let environment = Value::parse(&std::fs::read_to_string(
            root.join("support-environment.json"),
        )?)?;
        if let Value::Object(fields) = environment {
            for (name, value) in fields {
                std::env::set_var(Value::Text(name).string()?, value.string()?);
            }
        }
        let mut support = openpilot_carrot_server::support_terminal::Config::original(
            writable,
            Arc::clone(&terminal),
        )?;
        support.web = std::env::current_dir()?.join("openpilot/selfdrive/carrot/web");
        writable.support_terminal = Some(
            openpilot_carrot_server::support_terminal::Service::start(support)?,
        );
    }
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let terminal_control = Arc::clone(&terminal);
    let support_control = writable.support_terminal.clone();
    let stream_history = writable
        .stream_proxy
        .as_ref()
        .map(|service| service.history.clone());
    let input_worker = std::thread::spawn(move || {
        let line = io::stdin().lock().lines().next();
        if line
            .as_ref()
            .is_some_and(|line| line.as_ref().is_ok_and(|line| line == "force"))
        {
            terminal_control.force();
            if let Some(support) = support_control {
                support.force();
            }
        }
        let _accepted = stop.send(());
    });
    let serve = openpilot_carrot_server::http::serve(app, listener, async {
        let _stopped = stopped.await;
    });
    tokio::task::LocalSet::new().run_until(serve).await?;
    input_worker.join().map_err(|_| "fixture input failed")?;
    if root.join("stream-target.json").is_file() {
        if let Some(history) = stream_history {
            std::fs::write(
                root.join("stream-history.json"),
                history.snapshot()?.encode()?,
            )?;
        }
    }
    println!("{{\"app_cleanup\":true,\"terminal_reaped\":true}}");
    io::stdout().flush()?;
    Ok(())
}
