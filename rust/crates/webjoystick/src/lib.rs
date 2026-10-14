//! Native port of tools/bodyteleop/web.py, original tree 2256431333346686bfc27911b300dc6a9a07b166.
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{body::Body, Method, Request, Response};
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use std::{convert::Infallible, path::Path, sync::Arc};
use tokio::{net::TcpListener, sync::watch, task::JoinSet};
use tokio_rustls::TlsAcceptor;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

fn response(status: u16, mime: &str, bytes: &[u8], head: bool) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", mime)
        .header("content-length", bytes.len())
        .body(Full::new(if head {
            Bytes::new()
        } else {
            Bytes::copy_from_slice(bytes)
        }))
        .unwrap()
}
fn asset(path: &str) -> Option<(&'static str, &'static [u8])> {
    Some(match path {
        "/" | "/static/index.html" => (
            "text/html; charset=utf-8",
            include_bytes!("../assets/static/index.html"),
        ),
        "/static/main.css" => ("text/css", include_bytes!("../assets/static/main.css")),
        "/static/poster.png" => ("image/png", include_bytes!("../assets/static/poster.png")),
        "/static/js/controls.js" => (
            "application/javascript",
            include_bytes!("../assets/static/js/controls.js"),
        ),
        "/static/js/jsmain.js" => (
            "application/javascript",
            include_bytes!("../assets/static/js/jsmain.js"),
        ),
        "/static/js/plots.js" => (
            "application/javascript",
            include_bytes!("../assets/static/js/plots.js"),
        ),
        "/static/js/webrtc.js" => (
            "application/javascript",
            include_bytes!("../assets/static/js/webrtc.js"),
        ),
        _ => return None,
    })
}

pub async fn handle<B>(request: Request<B>, upstream: &str) -> Response<Full<Bytes>>
where
    B: Body<Data = Bytes> + Send,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let method = request.method();
    let path = request.uri().path();
    let head = *method == Method::HEAD;
    if *method == Method::GET || head {
        if path == "/ping" {
            return response(200, "text/plain; charset=utf-8", b"pong", head);
        }
        if let Some((mime, bytes)) = asset(path) {
            return response(200, mime, bytes, head);
        }
    }
    if path != "/offer" || *method != Method::POST {
        let known = path == "/ping" || path == "/offer" || asset(path).is_some();
        return response(
            if known { 405 } else { 404 },
            "text/plain; charset=utf-8",
            b"",
            head,
        );
    }
    let Ok(body) = Limited::new(request.into_body(), 1024 * 1024)
        .collect()
        .await
    else {
        return response(
            413,
            "text/plain; charset=utf-8",
            b"request body too large",
            false,
        );
    };
    let result = async {
        let params: serde_json::Value = serde_json::from_slice(&body.to_bytes())?;
        let sdp = params.get("sdp").ok_or("missing sdp")?;
        let offer = serde_json::json!({"sdp":sdp,"cameras":["driver"],
            "bridge_services_in":["testJoystick"],"bridge_services_out":["carState"]});
        let result = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()?
            .post(upstream)
            .json(&offer)
            .send()
            .await?;
        if result.status() != 200 {
            return Err::<Vec<u8>, Error>("webrtcd rejected offer".into());
        }
        let content_type = result
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if content_type != "application/json" && !content_type.ends_with("+json") {
            return Err("webrtcd answer is not JSON content".into());
        }
        let answer: serde_json::Value = result.json().await?;
        Ok::<_, Error>(serde_json::to_vec(&answer)?)
    }
    .await;
    match result {
        Ok(bytes) => response(200, "application/json; charset=utf-8", &bytes, false),
        Err(error) => {
            eprintln!("webjoystick offer: {error}");
            response(
                500,
                "text/plain; charset=utf-8",
                b"500 Internal Server Error",
                false,
            )
        }
    }
}

/// Reuse existing identities. Generate only if either original PEM file is absent.
pub fn tls_config(root: &Path, openssl: &Path) -> Result<Arc<rustls::ServerConfig>, Error> {
    let directory = root.join("openpilot/tools/bodyteleop");
    std::fs::create_dir_all(&directory)?;
    let cert = directory.join("cert.pem");
    let key = directory.join("key.pem");
    if !cert.exists() || !key.exists() {
        let result = std::process::Command::new(openssl)
            .args(["req", "-x509", "-newkey", "rsa:4096", "-nodes", "-out"])
            .arg(&cert)
            .arg("-keyout")
            .arg(&key)
            .args([
                "-days",
                "365",
                "-subj",
                "/C=US/ST=California/O=commaai/OU=comma body",
            ])
            .output()?;
        if !result.status.success() {
            return Err(format!(
                "Error creating SSL certificate: {}",
                String::from_utf8_lossy(&result.stderr)
            )
            .into());
        }
    }
    let certs =
        CertificateDer::pem_slice_iter(&std::fs::read(cert)?).collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_slice(&std::fs::read(key)?)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    Ok(Arc::new(
        rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(certs, key)?,
    ))
}

/// All accepted connections belong to the server and are cancelled/joined on shutdown.
pub async fn serve(
    listener: TcpListener,
    config: Arc<rustls::ServerConfig>,
    upstream: String,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let acceptor = TlsAcceptor::from(config);
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            changed = stop.changed() => { if changed.is_err() || *stop.borrow() {break;} }
            Some(_) = connections.join_next(), if !connections.is_empty() => (),
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let acceptor = acceptor.clone();
                let upstream = upstream.clone();
                connections.spawn(async move {
                    let Ok(stream) = acceptor.accept(stream).await else { return; };
                    let service = hyper::service::service_fn(move |request| {
                        let upstream = upstream.clone();
                        async move { Ok::<_, Infallible>(handle(request, &upstream).await) }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(hyper_util::rt::TokioIo::new(stream), service).await;
                });
            }
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    Ok(())
}
