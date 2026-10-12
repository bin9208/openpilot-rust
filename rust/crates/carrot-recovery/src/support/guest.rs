use super::{model::Session, Support};
use crate::{
    http::{
        registry,
        request::{ReadError, Request},
        response::Response,
    },
    Error,
};
use std::{
    io::BufReader,
    net::{TcpListener, TcpStream},
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

const ASSETS: [(&str, &str); 16] = [
    ("design-tokens.css", "css/generated/design-tokens.css"),
    ("design-system.css", "css/generated/design-system.css"),
    ("tokens.css", "css/tokens.css"),
    ("layout_tokens.css", "css/layout_tokens.css"),
    ("base.css", "css/base.css"),
    ("layout.css", "css/layout.css"),
    ("components.css", "css/components.css"),
    ("terminal.css", "css/generated/terminal.css"),
    (
        "terminal_typing_indicator.js",
        "js/generated/terminal-shared.js",
    ),
    ("guest.css", "css/generated/terminal-guest.css"),
    ("guest.js", "js/generated/terminal-guest.js"),
    ("xterm.css", "css/vendor/xterm.css"),
    ("xterm.js", "js/vendor/xterm.js"),
    ("xterm-addon-shim.js", "js/vendor/xterm-addon-shim.js"),
    ("xterm-addon-webgl.js", "js/vendor/xterm-addon-webgl.js"),
    ("xterm-addon-canvas.js", "js/vendor/xterm-addon-canvas.js"),
];

pub(super) fn start(service: Arc<Support>, session: Arc<Session>) -> Result<String, Error> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let base = format!("http://{}", listener.local_addr()?);
    session.data()?.local_url = format!("{base}/support/terminal/{}", session.id);
    let owner = Arc::clone(&service);
    service.spawn(move || {
        while !session.stopped.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((socket, _)) => {
                    let id = owner.next.fetch_add(1, Ordering::Relaxed);
                    let inserted = || -> Result<bool, Error> {
                        let mut sockets = session
                            .sockets
                            .lock()
                            .map_err(|_| Error::Source("support sockets poisoned".into()))?;
                        if session.stopped.load(Ordering::SeqCst) {
                            return Ok(false);
                        }
                        sockets.insert(id, socket.try_clone()?);
                        Ok(true)
                    }();
                    match inserted {
                        Ok(true) => {
                            let registration = registry::Registration {
                                id,
                                sockets: Arc::clone(&session.sockets),
                            };
                            let service = Arc::clone(&owner);
                            if let Err(error) = owner.spawn(move || {
                                let _registration = registration;
                                if let Err(error) = connection(socket, &service) {
                                    eprintln!("[recovery] support guest: {error}");
                                }
                            }) {
                                eprintln!("[recovery] support guest spawn: {error}");
                            }
                        }
                        Ok(false) => break,
                        Err(error) => eprintln!("[recovery] support guest register: {error}"),
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => {
                    eprintln!("[recovery] support accept: {error}");
                    break;
                }
            }
        }
    })?;
    Ok(base)
}

fn protected(mut response: Response) -> Response {
    response
        .headers
        .push(("X-Content-Type-Options".into(), "nosniff".into()));
    response
        .headers
        .push(("Referrer-Policy".into(), "no-referrer".into()));
    response
}

fn get(service: &Support, request: &Request) -> Response {
    let path = &request.path;
    if path == "/" || path.starts_with("/support/terminal/") {
        let sid = if path == "/" {
            ""
        } else {
            path.rsplit('/').next().unwrap_or_default()
        };
        let sid: String = sid
            .chars()
            .filter(|ch| ch.is_alphanumeric() || matches!(ch, '-' | '_'))
            .collect();
        let Ok(page) =
            std::fs::read_to_string(service.config.web.join("support_terminal/guest.html"))
        else {
            return Response::framework(404, "");
        };
        let page = page
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace("__SESSION_ID__", &sid);
        let mut response = protected(Response::bytes(
            200,
            "text/html; charset=utf-8",
            page.into_bytes(),
        ));
        let host: String = request
            .header("Host")
            .unwrap_or_default()
            .chars()
            .filter(|ch| ch.is_alphanumeric() || ".-:[]".contains(*ch))
            .collect();
        let connect = if host.is_empty() {
            "'self'".into()
        } else {
            format!("'self' ws://{host} wss://{host}")
        };
        response.headers.extend([
            ("Content-Security-Policy".into(),format!("default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src {connect}; img-src 'self' data:; font-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'")),
            ("X-Frame-Options".into(),"DENY".into()),("Permissions-Policy".into(),"camera=(), microphone=(), geolocation=(), usb=(), serial=(), hid=(), bluetooth=()".into()),
        ]);
        return response;
    }
    if path.starts_with("/support-terminal-assets/") {
        let name = path.rsplit('/').next().unwrap_or_default();
        if let Some((_, asset)) = ASSETS.iter().find(|(key, _)| *key == name) {
            if let Ok(bytes) = std::fs::read(service.config.web.join(asset)) {
                return protected(Response::bytes(
                    200,
                    if asset.ends_with(".css") {
                        "text/css; charset=utf-8"
                    } else {
                        "text/javascript; charset=utf-8"
                    },
                    bytes,
                ));
            }
        }
    }
    Response::framework(404, "")
}

fn connection(socket: TcpStream, service: &Support) -> Result<(), Error> {
    let mut output = socket.try_clone()?;
    let mut reader = BufReader::new(socket);
    loop {
        let request = match Request::read(&mut reader) {
            Ok(Some(request)) => request,
            Ok(None) => return Ok(()),
            Err(ReadError::Io(error)) => return Err(error.into()),
            Err(ReadError::Framework(status, message)) => {
                Response::framework(status, &message).send(&mut output, false)?;
                return Ok(());
            }
        };
        if matches!(request.method.as_str(), "GET" | "HEAD")
            && request.path.starts_with("/ws/support_terminal/")
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
                return Ok(());
            }
            return service.guest_socket(
                reader,
                request.header("Sec-WebSocket-Key").unwrap_or_default(),
                request.path.rsplit('/').next().unwrap_or_default(),
            );
        }
        let response = match request.method.as_str() {
            "GET" | "HEAD" => get(service, &request),
            _ => Response::framework(501, &format!("Unsupported method ('{}')", request.method)),
        };
        response.send(&mut output, request.method == "HEAD")?;
        if !request.persistent
            || response
                .headers
                .iter()
                .any(|(key, value)| key == "Connection" && value == "close")
        {
            return Ok(());
        }
    }
}
