use openpilot_webjoystick::{serve, tls_config, Error};
use std::{path::PathBuf, process::ExitCode};

fn run() -> Result<(), Error> {
    let mut args = std::env::args_os().skip(1);
    let mut root = None;
    let mut openssl = None;
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!("openpilot-webjoystick --root PATH --openssl PATH");
            return Ok(());
        }
        if arg == "--root" && root.is_none() {
            root = Some(PathBuf::from(args.next().ok_or("missing root")?));
        } else if arg == "--openssl" && openssl.is_none() {
            openssl = Some(PathBuf::from(args.next().ok_or("missing openssl")?));
        } else {
            return Err("unknown or duplicate argument".into());
        }
    }
    let root = root.ok_or("--root required")?;
    let openssl = openssl.ok_or("--openssl required")?;
    openpilot_params::Params::for_runtime()?.put_bool("JoystickDebugMode", true)?;
    let config = tls_config(&root, &openssl)?;
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async move {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:5000").await?;
        let (tx, rx) = tokio::sync::watch::channel(false);
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        let server = serve(listener, config, "http://localhost:5001/stream".into(), rx);
        tokio::pin!(server);
        tokio::select! {
            result = &mut server => result,
            _ = async { tokio::select! { _ = tokio::signal::ctrl_c() => (), _ = term.recv() => () } } => {
                let _ = tx.send(true);
                server.await
            }
        }
    })
}
fn main() -> ExitCode {
    match openpilot_managed_entry::runtime("openpilot.tools.bodyteleop.web", "webjoystick", || {
        run().map_err(|error| {
            openpilot_managed_entry::StepError::raised(std::io::Error::other(error))
        })
    }) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("webjoystick: {error}");
            ExitCode::FAILURE
        }
    }
}
