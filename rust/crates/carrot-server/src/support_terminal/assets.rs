use super::Config;
use crate::{
    http::{Body, RequestBody},
    http_response::{response, text},
    Error,
};
use hyper::{
    header::{self, HeaderValue},
    Request, Response, StatusCode,
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
fn preset() -> hyper::HeaderMap {
    let mut headers = hyper::HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("Referrer-Policy", HeaderValue::from_static("no-referrer"));
    headers
}
pub(super) async fn file(
    request: &Request<RequestBody>,
    config: &Config,
    name: &str,
) -> Result<Response<Body>, Error> {
    let head = request.method() == hyper::Method::HEAD;
    let Some((_, path)) = ASSETS.iter().find(|(asset, _)| *asset == name) else {
        return Ok(text(StatusCode::NOT_FOUND, "404: Not Found", head));
    };
    let path = config.web.join(path);
    if !path.is_file() {
        return Ok(text(StatusCode::NOT_FOUND, "404: Not Found", head));
    }
    crate::static_web::file_response_with_headers(&path, request, preset()).await
}
pub(super) async fn page(
    request: &Request<RequestBody>,
    config: &Config,
    id: &str,
) -> Result<Response<Body>, Error> {
    let head = request.method() == hyper::Method::HEAD;
    let bytes = tokio::fs::read(config.web.join("support_terminal/guest.html")).await?;
    let escaped = id
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;");
    let page = crate::request_text::decode(&bytes, "utf-8")?
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace("__SESSION_ID__", &escaped);
    let host: String = request
        .headers()
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .unwrap_or("")
        .chars()
        .filter(|character| character.is_alphanumeric() || ".-:[]".contains(*character))
        .collect();
    let connect = if host.is_empty() {
        "'self'".into()
    } else {
        format!("'self' ws://{host} wss://{host}")
    };
    let mut result = response(
        StatusCode::OK,
        page.into_bytes(),
        "text/html; charset=utf-8",
        head,
    );
    result.headers_mut().extend(preset());
    result.headers_mut().insert("Content-Security-Policy", HeaderValue::try_from(format!("default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src {connect}; img-src 'self' data:; font-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'")).map_err(|error| Error::Source(error.to_string()))?);
    result.headers_mut().insert("Permissions-Policy", HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=(), usb=(), serial=(), hid=(), bluetooth=()"));
    result
        .headers_mut()
        .insert("X-Frame-Options", HeaderValue::from_static("DENY"));
    Ok(result)
}
