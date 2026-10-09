use base64::Engine;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{config::Config, http, live, params::Backend, Value};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    if input.get("mode").string().unwrap_or_default() == "codec" {
        let Value::Array(rows) = input.get("frames") else {
            return Err("expected frames".into());
        };
        let mut output = Vec::new();
        for row in rows {
            let bytes = std::fs::read(row.get("path").string()?)?;
            let service = row.get("service").string()?;
            let result = live::compact::encode(&service, &bytes, 65535);
            output.push(match result {
                Ok(bytes) => Value::object([
                    ("service", Value::text(&service)),
                    (
                        "bytes",
                        Value::text(&base64::engine::general_purpose::STANDARD.encode(bytes)),
                    ),
                ]),
                Err(error) => Value::object([
                    ("service", Value::text(&service)),
                    ("error", Value::text(&error.to_string())),
                ]),
            });
        }
        println!("{}", Value::Array(output).encode()?);
        return Ok(());
    }
    let params = if input.get("params").truth() {
        Some(openpilot_params::Params::for_runtime()?)
    } else {
        None
    };
    let composed = input.get("composed").truth();
    let service = if input.get("unavailable").truth() || composed {
        None
    } else {
        Some(live::Service::start(params.clone())?)
    };
    let app = if composed {
        let state = PathBuf::from(input.get("state").string()?);
        let mut config = Config::at(&state, &state, &state.join("settings.json"));
        config.repository = state.join("owned-repository");
        config.web = state.join("owned-web");
        config.shared_assets = state.join("owned-assets");
        config.legacy_state = state.join("absent-legacy");
        config.params_backup = state.join("owned-backup.json");
        for directory in [&config.repository, &config.web, &config.shared_assets] {
            std::fs::create_dir_all(directory)?;
        }
        let backend = match params {
            Some(params) => Backend::native(params, state),
            None => Backend::memory(state),
        };
        let mut app = http::Application::new(config, backend);
        let owned = Arc::get_mut(&mut app).ok_or("fixture Application alreadyshared")?;
        owned.heartbeat_params = None;
        owned.auto_update = None;
        owned.popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        if input.get("unavailable").truth() {
            owned.live = None;
            owned.live_error = None;
        }
        Some(app)
    } else {
        None
    };
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
        let mut line = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut line) {
            eprintln!("fixture input: {error}");
        }
        match stop.send(()) {
            Ok(()) | Err(()) => {}
        }
    });
    if let Some(app) = app {
        let observed = Arc::clone(&app);
        http::serve(app, listener, async move {
            match stopped.await {
                Ok(()) | Err(_) => {}
            }
            match Value::object([
                ("stopping", Value::Bool(true)),
                ("engaged", Value::Bool(observed.drive_engaged())),
            ])
            .encode()
            {
                Ok(encoded) => println!("{encoded}"),
                Err(error) => eprintln!("fixture phase: {error}"),
            }
            if let Err(error) = io::stdout().flush() {
                eprintln!("fixture phase flush: {error}");
            }
        })
        .await?;
        input_worker.join().map_err(|_| "fixture input panicked")?;
        return Ok(());
    }
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _=&mut stopped=>break,
            accepted=listener.accept()=>{
                let (socket,_)=accepted?;let service=service.clone();connections.spawn(async move {
                    let handler=service_fn(move |request| {let service=service.clone();async move {Ok::<_,std::convert::Infallible>(live::http::handle(http::decode_request(request),service,None).await)}});
                    if let Err(error)=hyper::server::conn::http1::Builder::new().preserve_raw_conditional_headers(true).close_after_response(true).serve_connection(TokioIo::new(socket),handler).with_upgrades().await {eprintln!("fixture connection: {error}");}
                });
            }
            _=connections.join_next(),if !connections.is_empty()=>{}
        }
    }
    drop(listener);
    if let Some(service) = &service {
        service.quiesce();
        service.shutdown().await?;
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    input_worker.join().map_err(|_| "fixture input panicked")?;
    Ok(())
}
