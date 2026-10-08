use crate::{
    http::{Application, Body},
    http_response::text,
    web_sound::{Context, Shutdown},
    Error, Value,
};
use bytes::Bytes;
use http_body_util::Full;
use hyper::{header, upgrade::OnUpgrade, Method, Request, Response, StatusCode};
use openpilot_hardware_info::HardwareInfo;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;

#[path = "../../carrot-navi/src/native/handshake.rs"]
mod handshake;

pub(crate) struct Launch {
    upgrade: OnUpgrade,
    context: Context,
}

#[derive(Clone)]
pub(crate) struct Sessions {
    sender: mpsc::UnboundedSender<Launch>,
    shutdown: watch::Receiver<Shutdown>,
}

impl Sessions {
    pub(crate) fn channel(
        shutdown: watch::Receiver<Shutdown>,
    ) -> (Self, mpsc::UnboundedReceiver<Launch>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self { sender, shutdown }, receiver)
    }
}

pub(crate) async fn run(mut launch: Launch) -> Result<(), Error> {
    if *launch.context.shutdown.borrow() != Shutdown::Running {
        return Ok(());
    }
    let upgraded = tokio::select! {
        upgraded = launch.upgrade => upgraded.map_err(|error| Error::Source(error.to_string()))?,
        _ = launch.context.shutdown.changed() => return Ok(()),
    };
    crate::web_sound::run(upgraded, launch.context).await
}

pub(crate) fn handle<T>(
    mut request: Request<T>,
    app: &Application,
    sessions: &Sessions,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    if !matches!(request.method(), &Method::GET | &Method::HEAD) {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static("GET,HEAD"));
        return response;
    }
    if let Some(error) = handshake::handshake_error(request.headers()) {
        return text(StatusCode::BAD_REQUEST, &error, head);
    }
    let params = match app.params.lock() {
        Ok(params) => params.native_params().cloned(),
        Err(_) => return failure(head),
    };
    let Some(key) = request.headers().get(header::SEC_WEBSOCKET_KEY) else {
        return failure(head);
    };
    let accept = match header::HeaderValue::try_from(derive_accept_key(key.as_bytes())) {
        Ok(accept) => accept,
        Err(_) => return failure(head),
    };
    let hardware = if std::path::Path::new("/TICI").is_file() {
        openpilot_hardware_info::Tici::default().get_device_type()
    } else {
        openpilot_hardware_info::Pc.get_device_type()
    };
    let launch = Launch {
        upgrade: hyper::upgrade::on(&mut request),
        context: Context {
            params,
            tizi: hardware.is_ok_and(|device| device == "tizi"),
            shutdown: sessions.shutdown.clone(),
        },
    };
    if sessions.sender.send(launch).is_err() {
        return failure(head);
    }
    let mut response = Response::new(Full::new(Bytes::new()));
    *response.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    response.headers_mut().insert(
        header::UPGRADE,
        header::HeaderValue::from_static("websocket"),
    );
    response.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("upgrade"),
    );
    response
        .headers_mut()
        .insert(header::SEC_WEBSOCKET_ACCEPT, accept);
    response
}

fn failure(head: bool) -> Response<Body> {
    text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    )
}
