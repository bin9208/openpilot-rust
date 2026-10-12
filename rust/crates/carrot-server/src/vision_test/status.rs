use super::{storage, Config};
use crate::{
    json_fields::set,
    youtube_test::{process, status::streams},
    Error, Value,
};
use num_traits::ToPrimitive;
use std::{
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    time::Duration,
};

pub(super) fn pid(value: &Value) -> Result<i32, Error> {
    if !value.truth() {
        return Ok(0);
    }
    value
        .int()?
        .to_i32()
        .ok_or_else(|| Error::Source("vision pid out of range".into()))
}
pub(super) fn port(config: &Config) -> bool {
    TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, config.port).into(),
        Duration::from_millis(200),
    )
    .is_ok()
}
pub fn get(config: &Config) -> Result<Value, Error> {
    let mut state = storage::read(config);
    let runner = pid(state.get("runner_pid"))?;
    let alive = process::alive(runner, &config.runner.pattern);
    let status = storage::text(state.get("status"));
    let status = if status.is_empty() {
        "stopped"
    } else {
        &status
    };
    set(
        &mut state,
        "status",
        Value::text(if alive || status == "error" {
            status
        } else {
            "stopped"
        }),
    )?;
    set(&mut state, "runner_pid", Value::integer(runner))?;
    set(&mut state, "runner_alive", Value::Bool(alive))?;
    let children = state.get("children").clone();
    let mut rows = Vec::new();
    for (name, spec) in &config.children {
        let id = pid(children.get(name))?;
        rows.push((
            name.chars().map(u32::from).collect(),
            Value::object([
                ("pid", Value::integer(id)),
                ("alive", Value::Bool(process::alive(id, &spec.pattern))),
            ]),
        ));
    }
    set(&mut state, "children", Value::Object(rows))?;
    set(
        &mut state,
        "vipc_streams",
        Value::Array(streams().into_iter().map(Value::integer).collect()),
    )?;
    set(&mut state, "webrtcd_port_open", Value::Bool(port(config)))?;
    set(
        &mut state,
        "log_path",
        Value::text(&config.log.to_string_lossy()),
    )?;
    let device = match config.params() {
        Ok(params) => {
            let bytes = params.get("DisableDM").ok().flatten().unwrap_or_default();
            let disable = match openpilot_beepd::integer(&bytes) {
                Ok(value) => value,
                Err(error) => crate::param_native::fatal("DisableDM", &error),
            };
            let boolean = |name| {
                params
                    .get(name)
                    .ok()
                    .flatten()
                    .is_some_and(|bytes| bytes == b"1")
            };
            Value::object([
                ("disable_dm", Value::integer(disable)),
                ("is_offroad", Value::Bool(boolean("IsOffroad"))),
                ("is_onroad", Value::Bool(boolean("IsOnroad"))),
            ])
        }
        Err(_) => Value::object([
            ("disable_dm", Value::Null),
            ("is_offroad", Value::Null),
            ("is_onroad", Value::Null),
        ]),
    };
    set(&mut state, "device", device)?;
    Ok(state)
}
pub(super) fn print(config: &Config) -> Result<i32, Error> {
    let status = get(config)?;
    let started = if status.get("started_at").truth() {
        status.get("started_at").float()?
    } else {
        0.
    };
    let seconds = if started != 0. {
        (storage::now() - started)
            .max(0.)
            .trunc()
            .to_u64()
            .unwrap_or(0)
    } else {
        0
    };
    println!(
        "[vision_test] {} elapsed={:02}:{:02}:{:02}",
        storage::text(status.get("status")),
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    );
    println!(
        "  runner           pid={} alive={}",
        shown_pid(status.get("runner_pid"))?,
        status.get("runner_alive").truth()
    );
    for (name, _) in &config.children {
        let child = status.get("children").get(name);
        println!(
            "  {name:<16} pid={} alive={}",
            shown_pid(child.get("pid"))?,
            child.get("alive").truth()
        );
    }
    let Value::Array(values) = status.get("vipc_streams") else {
        return Err(Error::Source("VisionIPC streams are not an array".into()));
    };
    let text = values
        .iter()
        .map(Value::string)
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");
    println!(
        "  VIPC streams     {}",
        if text.is_empty() { "-" } else { &text }
    );
    println!(
        "  webrtcd port     {} open={}",
        config.port,
        status.get("webrtcd_port_open").truth()
    );
    println!("  log              {}", config.log.display());
    let error = storage::text(status.get("error"));
    if !error.trim().is_empty() {
        println!("  error            {}", error.trim());
    }
    Ok(i32::from(!status.get("runner_alive").truth()))
}
fn shown_pid(value: &Value) -> Result<String, Error> {
    Ok(if value.truth() {
        value.string()?
    } else {
        "-".into()
    })
}
