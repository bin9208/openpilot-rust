use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    config::Config,
    http::{decode_request, serve, Application},
    params::Backend,
    xiaoge::{self, Online},
    Value,
};
use std::{
    io::{self, BufRead},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let repository = PathBuf::from(input.get("repository").string()?);
    let peer: SocketAddr = input.get("peer").string()?.parse()?;
    let online = Arc::new(Online::for_owned_peer(peer)?);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut line = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut line) {
            eprintln!("fixture stop input: {error}");
        }
        if stop.send(()).is_err() {
            eprintln!("fixture stop receiver closed");
        }
    });
    if input.get("composed").truth() {
        let config = Config::at(
            &repository,
            &repository.join("data"),
            &repository.join("settings.json"),
        );
        let backend = Backend::memory(config.state.clone());
        let mut app = Application::new(config, backend);
        let application = Arc::get_mut(&mut app).ok_or("fixture Application is already shared")?;
        application.xiaoge_online = online;
        application.popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        serve(app, listener, async {
            if stopped.await.is_err() {
                eprintln!("fixture stop sender closed");
            }
        })
        .await?;
        input_worker
            .join()
            .map_err(|_| "fixture input thread failed")?;
        return Ok(());
    }
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            joined = connections.join_next(), if !connections.is_empty() => {
                if let Some(Err(error)) = joined {
                    eprintln!("fixture connection task: {error}");
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let online = Arc::clone(&online);
                let repository = repository.clone();
                connections.spawn(async move {
                    let service = service_fn(move |request| {
                        let online = Arc::clone(&online);
                        let repository = repository.clone();
                        async move {
                            Ok::<_, std::convert::Infallible>(xiaoge::handle(decode_request(request), &repository, online).await)
                        }
                    });
                    if let Err(error) = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), service).await {
                        eprintln!("fixture connection: {error}");
                    }
                });
            }
        }
    }
    connections.shutdown().await;
    input_worker
        .join()
        .map_err(|_| "fixture input thread failed")?;
    Ok(())
}
