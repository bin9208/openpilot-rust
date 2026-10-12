pub(crate) mod registry;
pub mod request;
pub mod response;
use crate::{config::Config, support::Support, terminal::Terminal, Error, Value};
use response::Response;
use std::{
    io::BufReader,
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

fn mime(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "map" => "application/json; charset=utf-8",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

fn get(config: &Config, support: &Support, path: &str) -> Result<Response, Error> {
    let asset = match path {
        "/" | "/index.html" => Some((
            "text/html; charset=utf-8",
            include_str!(concat!(env!("OUT_DIR"), "/index.html")),
        )),
        "/recovery.js" => Some((
            "text/javascript; charset=utf-8",
            include_str!(concat!(env!("OUT_DIR"), "/recovery.js")),
        )),
        "/recovery-api.js" => Some((
            "text/javascript; charset=utf-8",
            include_str!(concat!(env!("OUT_DIR"), "/recovery-api.js")),
        )),
        _ => None,
    };
    if let Some((content_type, body)) = asset {
        return Ok(Response::bytes(200, content_type, body.as_bytes().into()));
    }
    if path == "/api/support_terminal/status" {
        return Response::json(200, support.snapshot()?);
    }
    if path == "/api/recovery/cwp" {
        let status = crate::cwp::status(config)?;
        return Response::json(if status.get("ok").truth() { 200 } else { 503 }, status);
    }
    if path == "/download/tmux.log" {
        if !config.tmux_log.is_file() {
            return Response::json(
                404,
                Value::object([
                    ("ok", Value::Bool(false)),
                    ("error", Value::text("file not found")),
                ]),
            );
        }
        let mut response = Response::bytes(
            200,
            "text/plain; charset=utf-8",
            std::fs::read(&config.tmux_log)?,
        );
        response.headers.push((
            "Content-Disposition".into(),
            "attachment; filename=tmux.log".into(),
        ));
        return Ok(response);
    }
    let relative = path.trim_start_matches('/');
    if relative.starts_with("css/") || relative.starts_with("js/") {
        if let (Ok(root), Ok(target)) = (
            config.web.canonicalize(),
            config.web.join(relative).canonicalize(),
        ) {
            if target.starts_with(root) && target.is_file() {
                if let Some(content_type) = target
                    .extension()
                    .and_then(|value| value.to_str())
                    .and_then(|value| mime(&value.to_lowercase()))
                {
                    let mut response = Response::bytes(200, content_type, std::fs::read(target)?);
                    response.cache = Some(true);
                    return Ok(response);
                }
            }
        }
    }
    Response::json(
        404,
        Value::object([
            ("ok", Value::Bool(false)),
            ("error", Value::text("not found")),
        ]),
    )
}

fn connection(
    socket: TcpStream,
    config: Arc<Config>,
    terminal: Arc<Terminal>,
    support: Arc<Support>,
) -> Result<(), Error> {
    let mut output = socket.try_clone()?;
    let mut reader = BufReader::new(socket);
    loop {
        let request = match request::Request::read(&mut reader) {
            Ok(Some(request)) => request,
            Ok(None) => break,
            Err(request::ReadError::Io(error)) => return Err(error.into()),
            Err(request::ReadError::Framework(status, message)) => {
                Response::framework(status, &message).send(&mut output, false)?;
                break;
            }
        };
        if matches!(request.method.as_str(), "GET" | "HEAD")
            && matches!(
                request.path.as_str(),
                "/ws/terminal_pty" | "/ws/support_terminal/owner"
            )
        {
            let error = if !request
                .header("Upgrade")
                .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
            {
                Some("expected websocket upgrade")
            } else if request
                .header("Sec-WebSocket-Key")
                .is_none_or(str::is_empty)
            {
                Some("missing websocket key")
            } else {
                None
            };
            if let Some(error) = error {
                Response::framework(400, error).send(&mut output, request.method == "HEAD")?;
                break;
            }
            let key = request.header("Sec-WebSocket-Key").unwrap_or_default();
            if request.path == "/ws/support_terminal/owner" {
                return support.owner_socket(reader, key);
            }
            let reset = request
                .query
                .get("reset")
                .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"));
            return crate::websocket::serve(reader, key, terminal, reset);
        }
        let response = match request.method.as_str() {
            "GET" | "HEAD" => get(&config, &support, &request.path)?,
            "POST" => match request.json(&mut reader) {
                Ok(payload) if request.path == "/api/support_terminal/start" => {
                    Response::json(200, support.start(&payload)?)?
                }
                Ok(_) if request.path == "/api/support_terminal/stop" => {
                    Response::json(200, support.stop("user")?)?
                }
                Ok(_) if request.path.starts_with("/api/support_terminal/commands/") => {
                    let parts: Vec<_> = request
                        .path
                        .trim_start_matches("/api/support_terminal/commands/")
                        .split('/')
                        .collect();
                    match parts.as_slice() {
                        [id, decision]
                            if !id.is_empty() && matches!(*decision, "approve" | "reject") =>
                        {
                            let result = support.decide(id, *decision == "approve")?;
                            Response::json(
                                if result.get("ok").truth() { 200 } else { 400 },
                                result,
                            )?
                        }
                        _ => Response::json(
                            404,
                            Value::object([
                                ("ok", Value::Bool(false)),
                                ("error", Value::text("not found")),
                            ]),
                        )?,
                    }
                }
                Ok(payload) if request.path == "/api/action" => {
                    let action = if payload.get("action").truth() {
                        payload.get("action").string()?
                    } else {
                        String::new()
                    };
                    Response::json(
                        200,
                        crate::actions::run(&config, &terminal, action.trim(), &payload)?,
                    )?
                }
                Ok(payload) if request.path == "/api/recovery/cwp" => {
                    match payload.get("enabled") {
                        Value::Bool(enabled) => {
                            let result = crate::cwp::set_enabled(&config, *enabled)?;
                            Response::json(
                                if result.get("ok").truth() { 200 } else { 409 },
                                result,
                            )?
                        }
                        _ => Response::json(
                            400,
                            Value::object([
                                ("ok", Value::Bool(false)),
                                ("error", Value::text("enabled must be boolean")),
                            ]),
                        )?,
                    }
                }
                Ok(payload) if request.path == "/api/terminal/input" => {
                    let result = if payload.get("data").truth() {
                        payload.get("data").string().map_err(Error::from)
                    } else {
                        Ok(String::new())
                    }
                    .and_then(|text| terminal.write_text(&format!("{text}\r")));
                    Response::json(
                        200,
                        match result {
                            Ok(()) => Value::object([("ok", Value::Bool(true))]),
                            Err(error) => Value::object([
                                ("ok", Value::Bool(false)),
                                ("error", Value::text(&error.to_string())),
                            ]),
                        },
                    )?
                }
                Ok(_) => Response::json(
                    404,
                    Value::object([
                        ("ok", Value::Bool(false)),
                        ("error", Value::text("not found")),
                    ]),
                )?,
                Err(error) => Response::json(
                    400,
                    Value::object([
                        ("ok", Value::Bool(false)),
                        ("error", Value::text(&error.to_string())),
                    ]),
                )?,
            },
            _ => Response::framework(501, &format!("Unsupported method ('{}')", request.method)),
        };
        response.send(&mut output, request.method == "HEAD")?;
        if !request.persistent
            || response
                .headers
                .iter()
                .any(|(name, value)| name == "Connection" && value == "close")
        {
            break;
        }
    }
    Ok(())
}

pub fn serve(config: Config, listener: TcpListener, stop: Arc<AtomicBool>) -> Result<(), Error> {
    listener.set_nonblocking(true)?;
    let terminal = Terminal::new(config.clone())?;
    let support = Support::new(config.clone(), Arc::clone(&terminal))?;
    let config = Arc::new(config);
    let sockets = Arc::new(Mutex::new(std::collections::BTreeMap::new()));
    let mut workers = Vec::new();
    let mut next = 0_u64;
    while !stop.load(Ordering::SeqCst) {
        while let Some(index) = workers.iter().position(thread::JoinHandle::is_finished) {
            let worker = workers.swap_remove(index);
            if worker.join().is_err() {
                eprintln!("[recovery] completed request worker panicked");
            }
        }
        match listener.accept() {
            Ok((socket, _)) => {
                next = next.wrapping_add(1);
                sockets
                    .lock()
                    .map_err(|_| Error::Source("socket registry poisoned".into()))?
                    .insert(next, socket.try_clone()?);
                let registration = registry::Registration {
                    id: next,
                    sockets: Arc::clone(&sockets),
                };
                let config = Arc::clone(&config);
                let terminal = Arc::clone(&terminal);
                let support = Arc::clone(&support);
                workers.push(thread::spawn(move || {
                    let _registration = registration;
                    if let Err(error) = connection(socket, config, terminal, support) {
                        eprintln!("[recovery] connection: {error}");
                    }
                }));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) => {
                eprintln!("[recovery] accept: {error}");
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
    drop(listener);
    let mut cleanup = registry::close(&sockets);
    if let Err(error) = support.shutdown() {
        eprintln!("[recovery] support cleanup: {error}");
        cleanup = Err(error);
    }
    for worker in workers {
        if worker.join().is_err() {
            cleanup = Err(Error::Source("request worker panicked".into()));
        }
    }
    if let Err(error) = crate::actions::shutdown() {
        eprintln!("[recovery] action cleanup: {error}");
        cleanup = Err(error);
    }
    if let Err(error) = terminal.shutdown() {
        eprintln!("[recovery] terminal cleanup: {error}");
        cleanup = Err(error);
    }
    cleanup
}
