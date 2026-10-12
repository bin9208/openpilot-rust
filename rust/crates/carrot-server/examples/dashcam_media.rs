use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    config::Config,
    dashcam::{self, Media},
    http::Application,
    params::Backend,
    Value,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

fn media(input: &Value) -> Result<Arc<Media>, Box<dyn std::error::Error>> {
    Ok(Media::for_test(
        PathBuf::from(input.get("root").string()?),
        PathBuf::from(input.get("cache").string()?),
        PathBuf::from(input.get("ffmpeg").string()?),
    ))
}
fn policy(input: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let media = media(input)?;
    let segment = input.get("segment");
    let result = match input.get("operation").string()?.as_str() {
        "thumbnail" => media.thumbnail(segment).map(|path| (path, None)),
        "preview" => media.preview(segment).map(|path| (path, None)),
        "video" => media
            .video(segment)
            .map(|(path, content_type)| (path, Some(content_type))),
        _ => return Err("unknown owned fixture operation".into()),
    };
    Ok(match result {
        Ok((path, content_type)) => Value::object([
            ("path", Value::text(&path.to_string_lossy())),
            (
                "content_type",
                content_type.map_or(Value::Null, Value::text),
            ),
        ]),
        Err(dashcam::Failure::Http { status, message }) => Value::object([
            ("status", Value::integer(status)),
            ("message", Value::text(&message)),
        ]),
        Err(error) => Value::object([("error", Value::text(&error.to_string()))]),
    })
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    if !input.get("http").truth() {
        println!("{}", policy(&input)?.encode()?);
        return Ok(());
    }
    let media = media(&input)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    io::stdout().flush()?;
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut line = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut line) {
            eprintln!("fixture input: {error}");
        }
        let _ = stop.send(());
    });
    if input.get("composed").truth() {
        let state = PathBuf::from(input.get("state").string()?);
        let mut config = Config::at(&state, &state, &state.join("settings.json"));
        config.web = state.join("owned-web");
        config.shared_assets = state.join("owned-assets");
        config.legacy_state = state.join("absent-legacy");
        config.params_backup = state.join("owned-backup.json");
        std::fs::create_dir_all(&config.web)?;
        std::fs::create_dir_all(&config.shared_assets)?;
        let mut app = Application::new(config, Backend::memory(state));
        let app_mut = Arc::get_mut(&mut app).ok_or("fixture Application already shared")?;
        app_mut.popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        app_mut.dashcam_media = Arc::clone(&media);
        openpilot_carrot_server::http::serve(app, listener, async move {
            let _ = stopped.await;
        })
        .await?;
    } else {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    let media = Arc::clone(&media);
                    connections.spawn(async move {
                        let handler = service_fn(move |request| { let media = Arc::clone(&media); async move { Ok::<_,std::convert::Infallible>(dashcam::media_handle(openpilot_carrot_server::http::decode_request(request),media).await) } });
                        if let Err(error) = hyper::server::conn::http1::Builder::new().preserve_raw_conditional_headers(true).close_after_response(true).serve_connection(TokioIo::new(stream),handler).await { eprintln!("fixture connection: {error}"); }
                    });
                }
            }
        }
        connections.abort_all();
        while let Some(result) = connections.join_next().await {
            if let Err(error) = result {
                if !error.is_cancelled() {
                    return Err(error.into());
                }
            }
        }
    }
    input_worker.join().map_err(|_| "fixture input failed")?;
    Ok(())
}
