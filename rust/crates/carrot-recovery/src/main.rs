use openpilot_carrot_recovery::{config::Config, http, Error};
use std::{
    net::TcpListener,
    sync::{atomic::AtomicBool, Arc},
};

fn run() -> Result<(), Error> {
    let original: Vec<_> = std::env::args_os().skip(1).collect();
    if let Some(code) = openpilot_carrot_recovery::actions::internal_command(&original)? {
        std::process::exit(code);
    }
    let mut host = "0.0.0.0".to_owned();
    let mut port = 6999_u16;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--host" => {
                host = arguments
                    .next()
                    .ok_or_else(|| Error::Source("--host requires a value".into()))?
            }
            "--port" => {
                port = arguments
                    .next()
                    .ok_or_else(|| Error::Source("--port requires a value".into()))?
                    .parse()
                    .map_err(|_| Error::Source("invalid port".into()))?
            }
            "-h" | "--help" => {
                println!("usage: openpilot-carrot-recovery [--host HOST] [--port PORT]");
                return Ok(());
            }
            _ => return Err(Error::Source(format!("unrecognized argument: {argument}"))),
        }
    }
    let config = Config::original()?;
    let listener = TcpListener::bind((host.as_str(), port))?;
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(signal, Arc::clone(&stop))?;
    }
    println!(
        "[recovery] serving http://{host}:{port} cwd={} web={}",
        config.repository.display(),
        config.web.display()
    );
    let mut boot = openpilot_carrot_recovery::cwp::BootWorker::start(&config, port)?;
    let result = http::serve(config, listener, stop);
    let stopped = boot.stop();
    result.and(stopped)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
