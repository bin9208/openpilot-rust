use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    youtube_live::{
        self,
        network::Endpoint,
        service::{Config, Service},
    },
    Value,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = io::stdin()
        .lock()
        .lines()
        .next()
        .ok_or("missing fixture config")??;
    let input = Value::parse(&input)?;
    let root = PathBuf::from(input.get("owned_root").string()?);
    let params_path = PathBuf::from(input.get("params_root").string()?);
    if !params_path.starts_with(&root) {
        return Err("fixture Params root is not owned".into());
    }
    let params = openpilot_params::Params::open(&params_path, &input.get("prefix").string()?)?;
    let endpoint = url::Url::parse(&input.get("endpoint").string()?)?;
    let host = endpoint.host_str().ok_or("fixture host missing")?;
    let ip: std::net::IpAddr = host.parse()?;
    if !ip.is_loopback() {
        return Err("fixture recipients must be loopback".into());
    }
    let service = Service::start(Config {
        state_path: root.join("state/youtube_live.json"),
        secret_path: root.join("state/youtube_live_secret.json"),
        params: Some(params.clone()),
        endpoint: Endpoint {
            base: endpoint.to_string(),
            host: host.into(),
            port: endpoint.port().ok_or("fixture port missing")?,
        },
    })?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!(
        "{}",
        Value::object([
            ("port", Value::integer(listener.local_addr()?.port())),
            ("pid", Value::integer(std::process::id()))
        ])
        .encode()?
    );
    io::stdout().flush()?;
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let _line = io::stdin().lock().lines().next();
        let _stopped = stop.send(());
    });
    if input.get("application").truth() {
        application(
            AppFixture {
                root,
                params,
                service,
            },
            listener,
            stopped,
        )
        .await?;
        input_worker.join().map_err(|_| "fixture input failed")?;
        return Ok(());
    }
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?; let service = Arc::clone(&service);
                connections.spawn(async move {
                    let handler = service_fn(move |request| {
                        let service = Arc::clone(&service);
                        async move { Ok::<_, std::convert::Infallible>(youtube_live::http::handle(openpilot_carrot_server::http::decode_request(request), Some(service)).await) }
                    });
                    hyper::server::conn::http1::Builder::new().preserve_raw_conditional_headers(true).close_after_response(true)
                        .prefetch_buffered_body(true).application_continue(true).read_buf_exact_size(256*1024).serve_connection(TokioIo::new(stream), handler).await
                });
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    service.finish().await?;
    input_worker.join().map_err(|_| "fixture input failed")?;
    Ok(())
}

struct AppFixture {
    root: PathBuf,
    params: openpilot_params::Params,
    service: Arc<Service>,
}
async fn application(
    fixture: AppFixture,
    listener: tokio::net::TcpListener,
    stopped: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let AppFixture {
        root,
        params,
        service,
    } = fixture;
    let web = root.join("web");
    std::fs::create_dir_all(&web)?;
    std::fs::write(web.join("index.html"), "owned YouTube Application")?;
    let settings = root.join("settings.json");
    std::fs::write(&settings, "{}")?;
    let mut config =
        openpilot_carrot_server::config::Config::at(&std::env::current_dir()?, &root, &settings);
    config.web = web;
    config.legacy_state = root.join("legacy-state");
    config.params_backup = root.join("params-backup.json");
    // Other services have no Params, so this fixture never starts their external
    // recipients. Install the already-owned YouTube provider before serving.
    let mut app = openpilot_carrot_server::http::Application::new(
        config,
        openpilot_carrot_server::params::Backend::memory(root.join("state")),
    );
    let writable = Arc::get_mut(&mut app).ok_or("fixture Application shared before setup")?;
    *writable
        .params
        .get_mut()
        .map_err(|_| "fixture Params lock poisoned")? =
        openpilot_carrot_server::params::Backend::native(params, root.join("state"));
    writable.youtube_live = Some(service);
    openpilot_carrot_server::http::serve(app, listener, async {
        let _stopped = stopped.await;
    })
    .await?;
    Ok(())
}
