#[path = "tools_git_status/fixture.rs"]
mod fixture;

use hyper::{body::Incoming, service::service_fn, Request};
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    config::Config,
    git_state::Store,
    git_status::{Repository, Service},
    http::{serve, Application},
    params::Backend,
    tools_git_status, Value,
};
use std::{
    convert::Infallible,
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let root: PathBuf = input.get("root").string()?.into();
    let service = Service::with_clock(
        Repository {
            directory: input.get("repo").string()?.into(),
            lock: input.get("lock").string()?.into(),
            launcher: input.get("launcher").string()?.into(),
        },
        || 1000.0,
    );
    let store = Arc::new(Store::new(root.join("data/state")));
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    io::stdout().flush()?;
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::sync_channel(1);
    let worker = fixture::input(stop, released, root.join("descriptor-probe"));
    let served = if input.get("composed").truth() {
        let mut config = Config::at(&root, &root.join("data"), &root.join("settings.json"));
        config.web = root.join("web");
        config.shared_assets = root.join("assets");
        config.training_assets = root.join("assets/training");
        config.legacy_state = root.join("legacy");
        config.params_backup = root.join("params-backup.json");
        if input.get("invalid_web").truth() {
            config.web = root.join("missing-web");
        }
        let backend = Backend::memory(config.state.clone());
        let mut app = if input.get("inactive").truth() {
            Application::new(config, backend)
        } else {
            Application::with_git_status(config, backend, Arc::clone(&service))
        };
        Arc::get_mut(&mut app)
            .ok_or("fixture Application is already shared")?
            .popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        eprintln!(
            "fixture Git service installed: {}",
            app.git_status.is_some()
        );
        serve(app, listener, async {
            let _ = (&mut stopped).await;
        })
        .await
    } else {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    let service = Arc::clone(&service);
                    let store = Arc::clone(&store);
                    connections.spawn(async move {
                        let http = service_fn(move |request: Request<Incoming>| {
                            let service = Arc::clone(&service);
                            let store = Arc::clone(&store);
                            async move {
                                Ok::<_, Infallible>(tools_git_status::handle(&request, &service, &store).await)
                            }
                        });
                        hyper::server::conn::http1::Builder::new()
                            .close_after_response(true)
                            .serve_connection(TokioIo::new(stream), http).await
                    });
                }
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        service.wait_idle().await;
        Ok(())
    };
    release.send(())?;
    worker
        .join()
        .map_err(|_| "fixture input thread panicked")??;
    println!(
        "{}",
        Value::object([(
            "serve_error",
            served
                .as_ref()
                .err()
                .map_or(Value::Null, |error| Value::text(&error.to_string())),
        )])
        .encode()?
    );
    if input.get("expect_error").truth() {
        if served.is_ok() {
            return Err("owned serve-error fixture succeeded".into());
        }
    } else {
        served?;
    }
    Ok(())
}
