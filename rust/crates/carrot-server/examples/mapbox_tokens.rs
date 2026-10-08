use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    config::Config,
    http::Application,
    mapbox_tokens::{self, Online},
    params::Backend,
    Value,
};
use std::{
    io::{self, BufRead},
    path::PathBuf,
    sync::Arc,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let root = PathBuf::from(input.get("root").string()?);
    let state = PathBuf::from(input.get("state").string()?);
    let backend = Backend::native(
        openpilot_params::Params::for_runtime_at(&root)?,
        state.clone(),
    );
    let config = Config::at(&state, &state, &state.join("settings.json"));
    let app = Application::new(config, backend);
    let online = Arc::new(Online::for_test_endpoint(input.get("endpoint").string()?));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut line = String::new();
        io::stdin()
            .lock()
            .read_line(&mut line)
            .expect("fixture stop input");
        stop.send(()).expect("fixture receiver");
    });
    loop {
        tokio::select! {
            _=&mut stopped=>break,
            accepted=listener.accept()=> {
                let (stream,_)=accepted?;
                let app=app.clone();let online=online.clone();
                tokio::spawn(async move {
                    let service=service_fn(move |request| { let app=app.clone();let online=online.clone();async move { Ok::<_,std::convert::Infallible>(mapbox_tokens::handle(request,app,online).await) } });
                    if let Err(error)=hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream),service).await { eprintln!("fixture connection: {error}"); }
                });
            }
        }
    }
    input_worker.join().map_err(|_| "fixture input failed")?;
    Ok(())
}
