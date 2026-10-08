use openpilot_carrot_server::{
    config::Config,
    http::{serve, Application},
    params::Backend,
    Error,
};
use std::{env, path::PathBuf};

async fn shutdown() {
    let mut terminate =
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
    tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
}

fn affinity() {
    if cfg!(target_os = "linux") && std::path::Path::new("/TICI").is_file() {
        let result = (|| {
            let input = env::var("CARROT_WEB_CORES").unwrap_or_default();
            let mut cores = Vec::new();
            for part in input.split(',').filter(|part| !part.trim().is_empty()) {
                cores.push(part.trim().parse::<usize>().map_err(|_| ())?);
            }
            if cores.is_empty() {
                cores = vec![0, 1, 2, 3];
            }
            let mut mask = nix::sched::CpuSet::new();
            for core in cores {
                mask.set(core).map_err(|_| ())?;
            }
            nix::sched::sched_setaffinity(nix::unistd::Pid::from_raw(0), &mask).map_err(|_| ())
        })();
        if result.is_err() {
            println!("[carrot_server] failed to set core affinity");
        }
    }
}

async fn run() -> Result<(), Error> {
    affinity();
    let repository =
        std::fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))?;
    let mut config = Config::from_environment(&repository);
    let mut host = "0.0.0.0".to_owned();
    let mut port = 7000_u16;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let (name, inline) = argument
            .split_once('=')
            .map_or((argument.as_str(), None), |(name, value)| {
                (name, Some(value))
            });
        let mut value = || {
            inline
                .map(str::to_owned)
                .or_else(|| arguments.next())
                .ok_or_else(|| Error::Source(format!("argument {name}: expected one argument")))
        };
        match name {
            "--host" => host = value()?,
            "--port" => {
                port = value()?
                    .parse()
                    .map_err(|_| Error::Source("argument --port: invalid int value".into()))?
            }
            "--settings" => config.settings = value()?.into(),
            "-h" | "--help" => {
                println!("usage: openpilot-carrot-server [-h] [--host HOST] [--port PORT] [--settings SETTINGS]");
                return Ok(());
            }
            _ => return Err(Error::Source(format!("unrecognized arguments: {argument}"))),
        }
    }
    config.validate()?;
    openpilot_carrot_server::static_web::StaticWeb::new(config.clone()).validate()?;
    if !config.settings.exists() {
        println!(
            "[WARN] settings file not found: {}",
            config.settings.display()
        );
    }
    println!(
        "[carrot_server] serving {} on {host}:{port}",
        config.web.display()
    );
    config.migrate_legacy_state();
    let backend = Backend::native(
        openpilot_params::Params::for_runtime()?,
        config.state.clone(),
    );
    let listener = tokio::net::TcpListener::bind((host.as_str(), port)).await?;
    serve(Application::new(config, backend), listener, shutdown()).await
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
