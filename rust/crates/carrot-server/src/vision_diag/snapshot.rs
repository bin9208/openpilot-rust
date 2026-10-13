use super::{commands, processes, text, Config};
use crate::{json_fields::set, stream_proxy::History, Error, Value};
use openpilot_msgq::{VisionClient, VisionStream};
use std::{
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    time::Duration,
};

fn params(config: &Config) -> Result<Value, Error> {
    let Some(params) = config.params()? else {
        return Ok(Value::object([("available", Value::Bool(false))]));
    };
    let mut fields = vec![("available", Value::Bool(true))];
    for key in ["IsOffroad", "IsTakingSnapshot", "CarParamsPersistent"] {
        let value = match params.get(key) {
            Ok(value) => {
                let bytes = value.unwrap_or_default();
                if key == "CarParamsPersistent" {
                    Value::object([
                        ("present", Value::Bool(!bytes.is_empty())),
                        ("bytes", Value::integer(bytes.len())),
                    ])
                } else {
                    Value::Bool(bytes == b"1")
                }
            }
            Err(openpilot_params::Error::Io(_)) => {
                if key == "CarParamsPersistent" {
                    Value::object([
                        ("present", Value::Bool(false)),
                        ("bytes", Value::integer(0)),
                    ])
                } else {
                    Value::Bool(false)
                }
            }
            Err(error) => {
                Value::object([("error", Value::text(&text::trim(&error.to_string(), 800)))])
            }
        };
        fields.push((key, value));
    }
    Ok(Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.chars().map(u32::from).collect(), value))
            .collect(),
    ))
}
fn vipc() -> Value {
    match VisionClient::available_streams("camerad") {
        Ok(streams) => {
            let mut streams: Vec<_> = streams
                .into_iter()
                .map(|stream| match stream {
                    VisionStream::Road => 0,
                    VisionStream::Driver => 1,
                    VisionStream::WideRoad => 2,
                    VisionStream::Map => 3,
                })
                .collect();
            streams.sort_unstable();
            Value::object([
                ("available", Value::Bool(true)),
                (
                    "camerad_streams",
                    Value::Array(streams.into_iter().map(Value::integer).collect()),
                ),
            ])
        }
        Err(error) => Value::object([
            ("available", Value::Bool(false)),
            ("error", Value::text(&text::trim(&error.to_string(), 800))),
        ]),
    }
}
fn base(config: &Config, history: &History) -> Result<Value, Error> {
    let processes = processes::collect(config);
    let status = crate::vision_test::status::get(&config.vision).unwrap_or_else(|error| {
        Value::object([("error", Value::text(&text::trim(&error.to_string(), 800)))])
    });
    let now = chrono::Utc::now();
    let port_open = TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, config.vision.port).into(),
        Duration::from_millis(250),
    )
    .is_ok();
    let proc_net = Value::Object(
        ["tcp", "tcp6", "udp", "udp6"]
            .into_iter()
            .map(|name| {
                (
                    name.chars().map(u32::from).collect(),
                    Value::Array(
                        text::lines(&text::read(
                            &config.proc_root.join("net").join(name),
                            256_000,
                        ))
                        .iter()
                        .take(320)
                        .map(|line| Value::text(&text::trim(line, 800)))
                        .collect(),
                    ),
                )
            })
            .collect(),
    );
    let hostname = nix::unistd::gethostname()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let platform = format!(
        "{}-{}-{}",
        std::env::consts::OS,
        text::read(std::path::Path::new("/proc/sys/kernel/osrelease"), 256).trim(),
        std::env::consts::ARCH
    );
    Ok(Value::object([
        (
            "captured_at",
            Value::Float(
                match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                    Ok(time) => time.as_secs_f64(),
                    Err(time) => -time.duration().as_secs_f64(),
                },
            ),
        ),
        (
            "captured_at_iso",
            Value::text(&now.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        ),
        (
            "system",
            Value::object([
                ("hostname", Value::text(&hostname)),
                ("platform", Value::text(&platform)),
                ("python", Value::Null),
                ("runtime", Value::text("rust")),
                (
                    "executable",
                    Value::text(&std::env::current_exe()?.to_string_lossy()),
                ),
                (
                    "uptime_seconds",
                    Value::text(&text::trim(
                        &text::read(&config.proc_root.join("uptime"), 128),
                        800,
                    )),
                ),
                (
                    "loadavg",
                    Value::text(&text::trim(
                        &text::read(&config.proc_root.join("loadavg"), 128),
                        800,
                    )),
                ),
            ]),
        ),
        ("params", params(config)?),
        ("vipc", vipc()),
        (
            "ports",
            Value::object([("webrtcd_5001_open", Value::Bool(port_open))]),
        ),
        ("processes", processes),
        ("stream_proxy_history", history.snapshot()?),
        (
            "vision_test",
            Value::object([
                ("status", status),
                (
                    "log_path",
                    Value::text(&config.vision.log.to_string_lossy()),
                ),
                ("log_tail", text::tail(&config.vision.log, 240)),
            ]),
        ),
        ("proc_net", proc_net),
    ]))
}

pub(super) async fn collect(config: Config, history: History) -> Result<Value, Error> {
    let owned = config.clone();
    let mut snapshot = tokio::task::spawn_blocking(move || base(&owned, &history))
        .await
        .map_err(|error| Error::Source(error.to_string()))??;
    let (runner, _running) = config.runner();
    let sockets = commands::sockets(&runner, snapshot.get("processes")).await;
    set(&mut snapshot, "sockets", sockets)?;
    set(
        &mut snapshot,
        "journal_tail",
        commands::journal(&runner).await,
    )?;
    Ok(snapshot)
}
