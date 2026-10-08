use super::{dispatch_legacy, listener};
use crate::{
    ingress::{json, naver, TCP_MAX_CLIENTS, TCP_MAX_FRAME, TCP_TIMEOUT_SECONDS},
    native::{
        actor::{Action, Handle},
        clock,
        config::Config,
    },
    sources::Source,
};
use std::{
    io::Read,
    net::{SocketAddr, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub fn run(config: Config, handle: Handle) {
    let listener = match listener(config.bind, config.tcp_port, true, 5) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("carrot_man navigation listener: {e}");
            return;
        }
    };
    let clients = Arc::new(AtomicUsize::new(0));
    let mut token = 0_u64;
    while !handle.stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, peer)) => {
                if clients.load(Ordering::Relaxed) >= TCP_MAX_CLIENTS {
                    eprintln!("navigation ingress rejected: server_busy");
                    drop(stream);
                    continue;
                }
                clients.fetch_add(1, Ordering::Relaxed);
                token += 1;
                let handle = handle.clone();
                let client_count = Arc::clone(&clients);
                if let Err(error) = thread::Builder::new()
                    .name("carrot-navi-client".into())
                    .spawn(move || {
                        serve(stream, peer, token, &handle);
                        client_count.fetch_sub(1, Ordering::Relaxed);
                    })
                {
                    clients.fetch_sub(1, Ordering::Relaxed);
                    eprintln!("navigation ingress rejected: client_handler_error: {error}");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(error) => {
                eprintln!("navigation ingress rejected: listener_error: {error}");
                return;
            }
        }
    }
}

fn serve(mut stream: TcpStream, peer: SocketAddr, token: u64, handle: &Handle) {
    if handle.call(Action::PeerTcp(token, peer)).is_err() {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(TCP_TIMEOUT_SECONDS)));
    let _ = socket2::SockRef::from(&stream).set_keepalive(true);
    let session = format!("tmap-tcp-{}", uuid::Uuid::new_v4());
    let mut association: Option<(Source, String)> = None;
    let mut lost = false;
    let mut buffer = Vec::new();
    let mut done = false;
    while !done && !handle.stop.load(Ordering::Relaxed) {
        let remaining = (TCP_MAX_FRAME + 1).saturating_sub(buffer.len());
        if remaining == 0 {
            eprintln!("navigation ingress rejected: frame_too_large");
            break;
        }
        let mut input = vec![0; remaining.min(64 * 1024)];
        match stream.read(&mut input) {
            Ok(0) => {
                if buffer.is_empty() {
                    lost = true;
                } else {
                    eprintln!("navigation ingress rejected: unterminated_frame");
                }
                break;
            }
            Ok(length) => buffer.extend_from_slice(&input[..length]),
            Err(_) => {
                lost = true;
                break;
            }
        }
        while let Some(newline) = buffer.iter().position(|b| *b == b'\n') {
            if newline > TCP_MAX_FRAME {
                eprintln!("navigation ingress rejected: frame_too_large");
                done = true;
                break;
            }
            let frame = buffer[..newline].to_vec();
            buffer.drain(..newline + 1);
            let value = match json::parse(&frame, true) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("navigation ingress rejected: {}", e.0);
                    done = true;
                    break;
                }
            };
            if let Some(schema) = value.get("schema") {
                if !schema.text_eq("naver.navigation.v1") {
                    eprintln!("navigation ingress rejected: unsupported_schema");
                    done = true;
                    break;
                }
                let snapshot = match naver::parse(&value, clock::monotonic()) {
                    Ok(s) => s,
                    Err(_) => {
                        eprintln!("navigation ingress rejected: invalid_navigation");
                        done = true;
                        break;
                    }
                };
                let terminal = snapshot.lifecycle.terminal();
                association = Some((snapshot.source, snapshot.session_id.clone()));
                if handle.call(Action::Naver(Box::new(snapshot))).is_err() {
                    eprintln!("navigation ingress rejected: client_handler_error");
                    done = true;
                    break;
                }
                if terminal {
                    association = None;
                    done = true;
                    break;
                }
            } else {
                if value.get("rgdata").is_some_and(|v| v.is_object()) {
                    association = Some((Source::TmapLegacy, session.clone()));
                }
                if dispatch_legacy(&value, peer, &session, None, handle).is_err() {
                    eprintln!("navigation ingress rejected: client_handler_error");
                    done = true;
                    break;
                }
            }
        }
        if buffer.len() > TCP_MAX_FRAME {
            eprintln!("navigation ingress rejected: frame_too_large");
            break;
        }
    }
    if lost {
        if let Some((source, session)) = association {
            let _ = handle.call(Action::TransportLost(source, session, clock::monotonic()));
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
    let _ = handle.call(Action::ClearTcp(token));
}
