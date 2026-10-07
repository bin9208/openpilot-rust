use crate::{
    ingress::{json, peers::Fallback, IngressError},
    native::{
        actor::{Action, Handle},
        broadcast::local_ip,
        clock,
        config::Config,
    },
    Error,
};
use openpilot_logmessaged::{JsonValue, JsonView};
use std::{
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    sync::atomic::Ordering,
    thread,
    time::Duration,
};

fn usable(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !ip.is_unspecified()
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_multicast()
        && o[0] < 240
}
fn discovery_shaped(value: &JsonValue) -> bool {
    let type_shaped = value
        .get("type")
        .and_then(|v| v.to_utf8())
        .is_some_and(|v| v.starts_with("carrot.navigation.discover"));
    let exact = if let JsonView::Object(fields) = value.view() {
        fields.len() == 3
            && ["type", "source", "schema_version"]
                .iter()
                .all(|key| value.get(key).is_some())
    } else {
        false
    };
    type_shaped || exact
}

pub fn discovery(
    value: &JsonValue,
    local: Ipv4Addr,
    peer: SocketAddr,
) -> Result<Option<Vec<u8>>, IngressError> {
    if !discovery_shaped(value) {
        return Ok(None);
    }
    let JsonView::Object(fields) = value.view() else {
        return Err(IngressError("invalid_discovery"));
    };
    if fields.len() != 3
        || !value
            .get("type")
            .is_some_and(|v| v.text_eq("carrot.navigation.discover"))
        || !value.get("source").is_some_and(|v| v.text_eq("naver"))
        || !value
            .get("schema_version")
            .is_some_and(|v| matches!(v.view(), JsonView::Integer("1")))
    {
        return Err(IngressError("invalid_discovery"));
    }
    if !usable(local) {
        return Err(IngressError("invalid_local_ip"));
    }
    if peer.port() != 7705 || !matches!(peer.ip(),std::net::IpAddr::V4(ip)if usable(ip)) {
        return Err(IngressError("invalid_peer"));
    }
    Ok(Some(format!("{{\"type\":\"carrot.navigation.discover.response\",\"server\":\"{local}\",\"port\":7712,\"schema\":1,\"schema_version\":1,\"lease_ms\":2000}}").into_bytes()))
}

fn datagram(
    socket: &UdpSocket,
    bytes: &[u8],
    peer: SocketAddr,
    handle: &Handle,
) -> Result<(), Error> {
    let strict = json::parse(bytes, true);
    let value = match strict {
        Ok(value) => value,
        Err(error) => {
            let legacy = json::parse(bytes, false).map_err(|_| Error::Contract(error.0))?;
            if discovery_shaped(&legacy) {
                return Err(Error::Contract("invalid_discovery"));
            }
            legacy
        }
    };
    let local = match peer.ip() {
        std::net::IpAddr::V4(ip) => local_ip(ip).unwrap_or(Ipv4Addr::UNSPECIFIED),
        std::net::IpAddr::V6(_) => Ipv4Addr::UNSPECIFIED,
    };
    if let Some(response) = discovery(&value, local, peer).map_err(|e| Error::Contract(e.0))? {
        socket.send_to(&response, SocketAddr::new(peer.ip(), 7705))?;
        return Ok(());
    }
    handle.call(Action::PeerFallback(Fallback::Udp, peer))?;
    let now = clock::monotonic();
    let status = crate::owner::packet::status(&json::finite_value(&value), now)?;
    let session = super::legacy_http_session(peer, "udp")?;
    handle.call(Action::Udp(status, session, now))?;
    Ok(())
}

pub fn run(config: Config, handle: Handle) {
    while !handle.stop.load(Ordering::Relaxed) {
        let result = (|| {
            let socket = UdpSocket::bind((config.bind, config.udp_port))?;
            socket.set_read_timeout(Some(Duration::from_secs(10)))?;
            while !handle.stop.load(Ordering::Relaxed) {
                let mut bytes = [0; 4096];
                match socket.recv_from(&mut bytes) {
                    Ok((0, _)) => break,
                    Ok((length, peer)) => {
                        if let Err(error) = datagram(&socket, &bytes[..length], peer, &handle) {
                            eprintln!("navigation ingress rejected: invalid_navigation: {error}");
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        let _ = handle.call(Action::ClearFallback(Fallback::Udp));
                        thread::sleep(Duration::from_secs(1));
                    }
                    Err(error) => return Err(Error::Io(error)),
                }
            }
            Ok::<_, Error>(())
        })();
        let _ = handle.call(Action::ClearFallback(Fallback::Udp));
        if let Err(error) = result {
            eprintln!("carrot_man UDP retry: {error}");
            thread::sleep(Duration::from_secs(2));
        } else {
            thread::sleep(Duration::from_secs(1));
        }
    }
}
