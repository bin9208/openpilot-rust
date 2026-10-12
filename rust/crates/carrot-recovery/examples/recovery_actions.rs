use openpilot_carrot_recovery::{actions, config::Config, http, Error};
use std::{
    net::TcpListener,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

fn run() -> Result<(), Error> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if let Some(code) = actions::internal_command(&arguments)? {
        std::process::exit(code);
    }
    let [root, tmux_log, launcher, port] = arguments.as_slice() else {
        return Err(Error::Source(
            "usage: recovery_actions REPOSITORY TMUX_LOG LAUNCHER PORT".into(),
        ));
    };
    let repository = PathBuf::from(root);
    let config = Config {
        web: repository.join("openpilot/selfdrive/carrot/web"),
        data: repository.join("fixture-data"),
        params: repository.join("fixture-params"),
        repository,
        launcher: launcher.into(),
        tmux_log: tmux_log.into(),
    };
    let port: u16 = port
        .to_str()
        .ok_or_else(|| Error::Source("invalid port".into()))?
        .parse()
        .map_err(|_| Error::Source("invalid port".into()))?;
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!("owned recovery actions http://127.0.0.1:{port}");
    let stopped = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(signal, Arc::clone(&stopped))?;
    }
    http::serve(config, listener, stopped)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
