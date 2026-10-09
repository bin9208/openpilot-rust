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
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let terminal_control = Arc::clone(&terminal);
    let input_worker = std::thread::spawn(move || {
        let line = io::stdin().lock().lines().next();
        if line
            .as_ref()
            .is_some_and(|line| line.as_ref().is_ok_and(|line| line == "force"))
        {
            terminal_control.force();
        }
        let _accepted = stop.send(());
    });
    let serve = openpilot_carrot_server::http::serve(app, listener, async {
        let _stopped = stopped.await;
    });
    tokio::task::LocalSet::new().run_until(serve).await?;
    input_worker.join().map_err(|_| "fixture input failed")?;
    println!("{{\"app_cleanup\":true,\"terminal_reaped\":true}}");
    io::stdout().flush()?;
    Ok(())
}
