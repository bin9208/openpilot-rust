use super::{
    clock,
    config::Config,
    parameters::{self, Write, Writes},
    upload::{self, Upload},
};
use crate::Error;
use openpilot_messaging::{runtime::SubMaster, state::Options};
use openpilot_params::Params;
use std::{
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

static QUEUE_LOCK: Mutex<()> = Mutex::new(());
pub fn queue_exception(params: &Params, writes: &Writes, reason: &str) -> bool {
    let Ok(_guard) = QUEUE_LOCK.lock() else {
        return false;
    };
    match params.get("CarrotException") {
        Ok(current) if current.as_deref().is_none_or(|v| v.is_empty()) => {
            writes.send(Write::Exception(reason.into())).is_ok()
        }
        Ok(current) => current.as_deref() == Some(reason.as_bytes()),
        Err(_) => false,
    }
}
fn env_bool(name: &str, default: &str) -> bool {
    ["1", "true", "yes", "on"].contains(
        &std::env::var(name)
            .unwrap_or_else(|_| default.into())
            .trim()
            .to_lowercase()
            .as_str(),
    )
}
fn env_seconds(name: &str, default: &str) -> Result<f64, Error> {
    openpilot_runtime_core::python_float::parse(
        &std::env::var(name).unwrap_or_else(|_| default.into()),
    )
    .ok_or(Error::Contract("diagnostic delay"))
}
pub fn start(config: Config, stop: Arc<AtomicBool>, network: Arc<AtomicBool>) -> Result<(), Error> {
    let automatic = env_bool("CARROT_AUTO_ONROAD_DIAGNOSTICS", "1");
    let onroad_delay = env_seconds("CARROT_AUTO_ONROAD_TMUX_DELAY_SECONDS", "60")?;
    let can_delay = env_seconds("CARROT_CAN_ERROR_TMUX_DELAY_SECONDS", "5")?;
    thread::Builder::new()
        .name("carrot-commands".into())
        .spawn(move || {
            if let Err(error) = run(config, stop, network, automatic, onroad_delay, can_delay) {
                eprintln!("carrot_man command worker: {error}");
            }
        })?;
    Ok(())
}
#[path = "diagnostic_state.rs"]
mod state;
use state::{idle, State};
fn setup_socket(context: &zmq::Context, config: &Config) -> Result<zmq::Socket, Error> {
    let socket = context.socket(zmq::REP)?;
    socket.bind(&format!(
        "tcp://{}:{}",
        if config.bind.is_unspecified() {
            "*".into()
        } else {
            config.bind.to_string()
        },
        config.command_port
    ))?;
    Ok(socket)
}
fn run(
    config: Config,
    stop: Arc<AtomicBool>,
    network: Arc<AtomicBool>,
    automatic: bool,
    onroad_delay: f64,
    can_delay: f64,
) -> Result<(), Error> {
    let params = parameters::open(&config)?;
    let writes = Writes::new(&config)?;
    let upload = Upload {
        config: &config,
        params: &params,
    };
    let context = zmq::Context::new();
    let mut socket = setup_socket(&context, &config)?;
    let mut sub = SubMaster::for_runtime(&["carState", "radarState"], Options::default())?;
    let mut state = State::default();
    while !stop.load(Ordering::Relaxed) {
        let result = (|| {
            let now = clock::monotonic();
            let mut items = [socket.as_poll_item(zmq::POLLIN)];
            let deadline = Instant::now() + Duration::from_millis(100);
            let mut timeout = 100;
            loop {
                match zmq::poll(&mut items, timeout) {
                    Ok(_) => break,
                    Err(zmq::Error::EINTR) => {
                        timeout = deadline
                            .saturating_duration_since(Instant::now())
                            .as_millis() as i64;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            let value = if items[0].is_readable() {
                let bytes = socket.recv_bytes(zmq::DONTWAIT)?;
                let text =
                    std::str::from_utf8(&bytes).map_err(|_| Error::Contract("command UTF-8"))?;
                Some(
                    openpilot_logmessaged::JsonValue::parse(text)
                        .map_err(|_| Error::Contract("command JSON"))?,
                )
            } else {
                None
            };
            match value {
                None => idle(
                    &mut state,
                    &mut sub,
                    &upload,
                    &writes,
                    now,
                    network.load(Ordering::Relaxed),
                    automatic,
                    onroad_delay,
                    can_delay,
                )?,
                Some(value) if matches!(value.view(), openpilot_logmessaged::JsonView::Null) => {
                    idle(
                        &mut state,
                        &mut sub,
                        &upload,
                        &writes,
                        now,
                        network.load(Ordering::Relaxed),
                        automatic,
                        onroad_delay,
                        can_delay,
                    )?
                }
                Some(value) => {
                    if let Some(command) = value.get("echo_cmd") {
                        let command = command
                            .to_utf8()
                            .ok_or(Error::Contract("echo command type"))?;
                        let result = Command::new("sh").args(["-c", &command]).output()?;
                        let decode = |bytes: &[u8]| {
                            charset_norm::codecs::decode(
                                bytes,
                                "euc-kr",
                                charset_norm::codecs::Errors::Ignore,
                            )
                            .map_err(|_| Error::Contract("echo decoding"))
                        };
                        let (stdout, stderr) = match (
                            std::str::from_utf8(&result.stdout),
                            std::str::from_utf8(&result.stderr),
                        ) {
                            (Ok(out), Ok(err)) => (out.into(), err.into()),
                            _ => (decode(&result.stdout)?, decode(&result.stderr)?),
                        };
                        socket.send(serde_json::json!({"echo_cmd":command,"exitStatus":result.status.code().unwrap_or(-1),"result":stdout,"error":stderr}).to_string().as_bytes(),0)?;
                    } else if value.get("tmux_send").is_some() {
                        let captured = upload.capture();
                        let web = if captured {
                            upload.web("tmux_send", false)
                        } else {
                            None
                        };
                        let logs = if captured {
                            upload.carrot_logs("tmux_send", false)
                        } else {
                            None
                        };
                        let web_ok = upload::ok(web.as_ref());
                        let logs_ok = upload::ok(logs.as_ref());
                        let discord_ok =
                            captured && upload.discord("tmux_send", web_ok, web.as_ref(), false);
                        socket.send(serde_json::json!({"tmux_send":true,"result":if web_ok||logs_ok||discord_ok{"success"}else{"failed"},"web_ok":web_ok,"carrot_logs_ok":logs_ok,"discord_ok":discord_ok}).to_string().as_bytes(),0)?;
                    }
                }
            }
            Ok::<_, Error>(())
        })();
        if let Err(error) = result {
            eprintln!("carrot_man command error: {error}");
            drop(socket);
            thread::sleep(Duration::from_secs(1));
            socket = setup_socket(&context, &config)?;
        }
    }
    Ok(())
}
