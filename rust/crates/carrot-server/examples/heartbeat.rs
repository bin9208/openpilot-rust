use openpilot_carrot_server::{
    heartbeat::{Environment, LoopExit, Online, Service},
    params::Backend,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{BufRead, Write},
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::{sync::watch, task::JoinHandle};

#[derive(Deserialize)]
struct Fixture {
    params_root: PathBuf,
    has_params: bool,
    peer: SocketAddr,
    tls: bool,
    ips: Vec<String>,
    times: Vec<f64>,
}

struct LoopTask {
    stop: watch::Sender<bool>,
    task: JoinHandle<Result<LoopExit, openpilot_carrot_server::Error>>,
}

fn sequence<T: Clone + Send + 'static>(values: Vec<T>) -> Arc<dyn Fn() -> T + Send + Sync> {
    let last = values
        .last()
        .expect("owned observation sequence empty")
        .clone();
    let queue = Mutex::new((VecDeque::from(values), last));
    Arc::new(move || {
        let mut state = queue.lock().expect("owned observations poisoned");
        if let Some(value) = state.0.pop_front() {
            state.1 = value;
        }
        state.1.clone()
    })
}

fn backend(params: &Option<openpilot_params::Params>, state: &std::path::Path) -> Backend {
    params.as_ref().map_or_else(
        || Backend::memory(state.into()),
        |params| Backend::native(params.clone(), state.into()),
    )
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let fixture: Fixture = serde_json::from_str(&line)?;
    let params = fixture
        .has_params
        .then(|| openpilot_params::Params::open(&fixture.params_root, "d"))
        .transpose()?;
    let state = fixture.params_root.join("state");
    let environment = Environment {
        local_ip: sequence(fixture.ips),
        time: sequence(fixture.times),
    };
    let service = Service::with_dependencies(
        Online::for_owned_peer(fixture.peer, fixture.tls)?,
        environment,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let status = service.clone();
    let server = tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            let status = status.clone();
            tokio::spawn(async move {
                let handler = hyper::service::service_fn(move |request| {
                    let response = openpilot_carrot_server::heartbeat::handle(&request, &status);
                    async { Ok::<_, std::convert::Infallible>(response) }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(hyper_util::rt::TokioIo::new(socket), handler)
                    .await;
            });
        }
    });
    let (sender, mut commands) = tokio::sync::mpsc::channel(8);
    let reader = tokio::task::spawn_blocking(move || {
        for line in std::io::stdin().lock().lines() {
            if sender.blocking_send(line).is_err() {
                break;
            }
        }
    });
    let mut running: Option<LoopTask> = None;
    println!("{}", json!({"port": port}));
    std::io::stdout().flush()?;
    while let Some(line) = commands.recv().await {
        let command: Value = serde_json::from_str(&line?)?;
        let result = match command["operation"]
            .as_str()
            .ok_or("owned heartbeat operation missing")?
        {
            "register" => {
                let service = service.clone();
                let params = backend(&params, &state);
                let (ok, message) =
                    tokio::task::spawn_blocking(move || service.register(&params)).await?;
                json!([ok, message])
            }
            "start" => {
                let (stop, receiver) = watch::channel(false);
                let service = service.clone();
                let params = backend(&params, &state);
                running = Some(LoopTask {
                    stop,
                    task: tokio::spawn(async move { service.run_loop(params, receiver).await }),
                });
                Value::Null
            }
            "stop" => {
                let running = running.take().ok_or("owned heartbeat loop absent")?;
                let _ = running.stop.send(true);
                match running.task.await?? {
                    LoopExit::Returned => json!("returned"),
                    LoopExit::Cancelled => json!("cancelled"),
                }
            }
            "status" => serde_json::from_str(&service.snapshot()?.encode()?)?,
            "ip" => {
                let peer: SocketAddr = command["route_peer"]
                    .as_str()
                    .ok_or("owned route peer missing")?
                    .parse()?;
                let route = command["route_ok"].as_bool().unwrap_or(false);
                let hostname = command["hostname"].as_str();
                json!(openpilot_carrot_server::heartbeat::environment::select_ip(
                    || route
                        .then(
                            || openpilot_carrot_server::heartbeat::environment::owned_route_ip(
                                peer
                            )
                        )
                        .flatten(),
                    || hostname
                        .and_then(openpilot_carrot_server::heartbeat::environment::hostname_ip),
                ))
            }
            "verified_tls" => {
                let address = format!("https://{}", fixture.peer);
                json!(ureq::get(&address).call().is_ok())
            }
            _ => return Err("unknown owned heartbeat operation".into()),
        };
        println!("{}", json!({"result": result}));
        std::io::stdout().flush()?;
    }
    if let Some(running) = running {
        let _ = running.stop.send(true);
        let _ = running.task.await;
    }
    server.abort();
    reader.await?;
    Ok(())
}
