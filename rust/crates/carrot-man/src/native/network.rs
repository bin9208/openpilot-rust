use super::{
    actor::{Action, Handle},
    clock,
    config::Config,
};
use crate::{ingress::json, Error};
use std::{
    io::Read,
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::atomic::Ordering,
    thread,
    time::Duration,
};
#[path = "kisa.rs"]
mod kisa;
#[path = "network_tcp.rs"]
mod tcp;
#[path = "network_udp.rs"]
mod udp;
pub use kisa::{apply_kisa, Kisa};

pub fn listener(
    address: Ipv4Addr,
    port: u16,
    reuse: bool,
    backlog: i32,
) -> Result<TcpListener, Error> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    if reuse {
        socket.set_reuse_address(true)?;
    }
    socket.bind(&SocketAddr::from((address, port)).into())?;
    socket.listen(backlog)?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

pub fn start(config: Config, handle: Handle) -> Result<(), Error> {
    let c = config.clone();
    let h = handle.clone();
    thread::Builder::new()
        .name("carrot-udp".into())
        .spawn(move || udp::run(c, h))?;
    let c = config.clone();
    let h = handle.clone();
    thread::Builder::new()
        .name("carrot-navi-tcp".into())
        .spawn(move || tcp::run(c, h))?;
    let c = config.clone();
    let h = handle.clone();
    thread::Builder::new()
        .name("carrot-route".into())
        .spawn(move || route(c, h))?;
    thread::Builder::new()
        .name("carrot-kisa".into())
        .spawn(move || kisa::run(config, handle))?;
    Ok(())
}

fn route(config: Config, handle: Handle) {
    let listener = match listener(config.bind, config.route_port, false, 128) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("carrot_man route listener: {e}");
            return;
        }
    };
    while !handle.stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Err(error) = read_route(&mut stream, &handle) {
                    eprintln!("carrot_man route receive: {error}");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(e) => {
                eprintln!("carrot_man route listener: {e}");
                return;
            }
        }
    }
}
fn read_route(stream: &mut TcpStream, handle: &Handle) -> Result<(), Error> {
    let mut header = [0; 4];
    if stream.read_exact(&mut header).is_err() {
        return Ok(());
    }
    let length = usize::try_from(u32::from_be_bytes(header))
        .map_err(|_| Error::Contract("route frame length"))?;
    let mut bytes = Vec::new();
    let read = stream
        .take(u64::try_from(length).map_err(|_| Error::Contract("route frame length"))?)
        .read_to_end(&mut bytes)?;
    if read != length {
        return Ok(());
    }
    let mut points = Vec::new();
    let mut complete = true;
    for chunk in bytes.chunks(8) {
        if chunk.len() != 8 {
            complete = false;
            break;
        }
        let lon = f32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let lat = f32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        points.push((f64::from(lon), f64::from(lat)));
    }
    handle.call(Action::Route(points, complete))?;
    Ok(())
}

pub fn legacy_http_session(peer: SocketAddr, version: &str) -> Result<String, Error> {
    use blake2::{
        digest::{Update, VariableOutput},
        Blake2sVar,
    };
    let ip = openpilot_logmessaged::JsonValue::text(&peer.ip().to_string())
        .to_json()
        .map_err(|_| Error::Contract("legacy session JSON"))?;
    let version = openpilot_logmessaged::JsonValue::text(version)
        .to_json()
        .map_err(|_| Error::Contract("legacy session JSON"))?;
    let seed = format!("[{ip},{version}]");
    let mut hash = Blake2sVar::new(8).map_err(|_| Error::Contract("legacy session hash"))?;
    hash.update(seed.as_bytes());
    let mut digest = [0; 8];
    hash.finalize_variable(&mut digest)
        .map_err(|_| Error::Contract("legacy session hash"))?;
    Ok(format!(
        "tmap-http-{}",
        digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

pub fn dispatch_legacy(
    value: &openpilot_logmessaged::JsonValue,
    peer: SocketAddr,
    session: &str,
    version: Option<&str>,
    handle: &Handle,
) -> Result<(), Error> {
    let mut ordered_keys = if let openpilot_logmessaged::JsonView::Object(keys) = value.view() {
        keys.iter()
            .map(|(points, _)| {
                points
                    .iter()
                    .copied()
                    .filter_map(char::from_u32)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    for key in version
        .map(|_| "_tmap_version")
        .into_iter()
        .chain(["_navigation_session_id", "_navigation_received_mono_s"])
    {
        if !ordered_keys.iter().any(|name| name == key) {
            ordered_keys.push(key.into());
        }
    }
    let now = clock::monotonic();
    let mut value = json::finite_value(value);
    if let Some(map) = value.as_object_mut() {
        if let Some(version) = version {
            map.insert("_tmap_version".into(), version.into());
        }
        map.insert("_navigation_session_id".into(), session.into());
        map.insert("_navigation_received_mono_s".into(), now.into());
    }
    let mut frame = crate::owner::packet::frame(&value, now, clock::wall(), session)?;
    if frame.event_type == "unknown" {
        frame.record.summary["keys"] =
            serde_json::json!(ordered_keys.into_iter().take(10).collect::<Vec<_>>());
    }
    let _ = peer;
    handle.call(Action::Legacy(frame, session.into(), now))?;
    Ok(())
}
