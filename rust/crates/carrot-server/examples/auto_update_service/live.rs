use super::fixture::{Fixture, Input};
use openpilot_carrot_server::{
    auto_update_runtime::Runtime,
    http::{serve, Application},
    params::Backend,
    popular_values::Service,
    Error,
};
use std::{
    io::{self, BufRead},
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio::sync::{mpsc, watch};

pub async fn run(input: &Input, fixture: &Fixture) -> Result<(), Box<dyn std::error::Error>> {
    let runtime = Runtime::with_inputs(
        &fixture.config,
        Some(fixture.params.clone()),
        Arc::clone(&fixture.service),
        fixture.inputs.clone(),
    );
    let (stop, stopped) = watch::channel(false);
    let task = if input.mode == "app" {
        let mut app = Application::for_runtime(
            fixture.config.clone(),
            Backend::native(fixture.params.clone(), input.state.clone()),
            Arc::clone(&fixture.service),
        );
        let fields = Arc::get_mut(&mut app).ok_or("App already shared")?;
        fields.auto_update = Some(runtime);
        fields.heartbeat_params = None;
        fields.popular_values = Service::new(false);
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        println!("{{\"address\":\"{address}\"}}");
        tokio::task::spawn_local(async move {
            let mut stopped = stopped;
            serve(app, listener, async move {
                let _ = stopped.changed().await;
            })
            .await
        })
    } else if input.mode == "wait-reboot" {
        let mode = input.reboot_mode.clone();
        let head = input.head.clone();
        tokio::task::spawn_local(async move {
            match runtime.wait_reboot(&mode, &head, stopped).await {
                Ok(())
                | Err(openpilot_carrot_server::auto_update_pull::Failure::Command(
                    openpilot_carrot_server::git_status::Failure::Cancelled,
                )) => Ok(()),
                Err(error) => Err(Error::Source(error.to_string())),
            }
        })
    } else {
        tokio::task::spawn_local(async move {
            runtime
                .run_with_timing(stopped, Duration::from_secs(60), Duration::ZERO)
                .await;
            Ok(())
        })
    };
    let (sender, mut commands) = mpsc::unbounded_channel();
    let stdin = std::thread::spawn(move || -> io::Result<()> {
        for line in io::stdin().lock().lines() {
            if sender.send(line?).is_err() {
                break;
            }
        }
        Ok(())
    });
    println!("{{\"ready\":true}}");
    while let Some(line) = commands.recv().await {
        let command: serde_json::Value = serde_json::from_str(&line)?;
        if command["stop"].as_bool() == Some(true) {
            break;
        }
        if let Some(now) = command["now"].as_f64() {
            fixture.now.store(now.to_bits(), Ordering::SeqCst);
        }
        println!("{}", fixture.result()?.encode()?);
    }
    stop.send_replace(true);
    task.await??;
    stdin.join().map_err(|_| "stdin owner panicked")??;
    Ok(())
}
