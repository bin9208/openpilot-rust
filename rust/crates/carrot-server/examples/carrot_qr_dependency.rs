use openpilot_carrot_server::{
    config::Config,
    http::{serve, Application},
    param_qr::Codec,
    params::Backend,
    qr_dependency::Provider,
    Value,
};
use serde::Deserialize;
use std::{
    io::{self, BufRead},
    path::PathBuf,
    sync::Arc,
};

#[derive(Deserialize)]
struct Input {
    root: PathBuf,
    bundle: PathBuf,
    #[serde(default)]
    system_fallback: bool,
    #[serde(default)]
    http: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut first = String::new();
    io::stdin().read_line(&mut first)?;
    let input: Input = serde_json::from_str(&first)?;
    let active = input.root.join("native-deps/brotli");
    let provider = Provider::new(input.bundle, active, input.system_fallback);
    if input.http {
        let config = Config::at(&input.root, &input.root, &input.root.join("settings.json"));
        let backend = Backend::native(
            openpilot_params::Params::open(&input.root.join("params"), "d")?,
            config.state.clone(),
        );
        let mut app = Application::new(config, backend);
        let app_mut = Arc::get_mut(&mut app).ok_or("fixture App shared before setup")?;
        app_mut.qr_dependency = provider;
        app_mut.heartbeat_params = None;
        app_mut.auto_update = None;
        app_mut.popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        app_mut.network_refresh = false;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        println!(
            "{{\"ready\":true,\"port\":{}}}",
            listener.local_addr()?.port()
        );
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let reader = tokio::task::spawn_blocking(move || -> io::Result<()> {
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
            match stop.send(()) {
                Ok(()) | Err(()) => {}
            }
            Ok(())
        });
        serve(app, listener, async move {
            match stopped.await {
                Ok(()) | Err(_) => {}
            }
        })
        .await?;
        reader.await??;
    } else {
        let backend = Backend::native(
            openpilot_params::Params::open(&input.root.join("params"), "d")?,
            input.root.join("state"),
        );
        for line in io::stdin().lock().lines() {
            let line = line?;
            let value = Value::parse(&line)?;
            let response = match value.get("action").string()?.as_str() {
                "status" => provider.status(),
                "ensure" => provider.ensure(),
                "build" => Codec::with_provider(&backend, &provider).build(value.get("values"))?,
                "parse" => Codec::with_provider(&backend, &provider).parse(value.get("payload"))?,
                "inspect" => {
                    let codec = Codec::with_provider(&backend, &provider);
                    let qr = codec.build(value.get("values"))?;
                    Value::object([
                        ("qr", qr),
                        (
                            "maps",
                            Value::text(&std::fs::read_to_string("/proc/self/maps")?),
                        ),
                    ])
                }
                action => return Err(format!("unknown action: {action}").into()),
            };
            println!("{}", response.encode()?);
        }
    }
    Ok(())
}
