use super::{html, manifest, StaticWeb};
use crate::{http::Body, http_response::response, Error, Value};
use hyper::{header, Response, StatusCode};
use std::{fs, sync::Arc, time::Duration};

const RECOVERY: &str = "<!doctype html>\n<html lang=\"ko\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n  <meta http-equiv=\"refresh\" content=\"1\">\n  <title>Carrot Web</title>\n  <style>\n    :root { color-scheme: dark; }\n    body {\n      align-items: center;\n      background: #0b1016;\n      color: #e8edf3;\n      display: flex;\n      font: 600 16px/1.5 system-ui, sans-serif;\n      justify-content: center;\n      margin: 0;\n      min-height: 100vh;\n    }\n    main { text-align: center; }\n    i {\n      animation: spin .8s linear infinite;\n      border: 3px solid #34404d;\n      border-radius: 50%;\n      border-top-color: #ff9f5a;\n      display: block;\n      height: 28px;\n      margin: 0 auto 16px;\n      width: 28px;\n    }\n    small { color: #8d99a6; display: block; font-weight: 500; margin-top: 4px; }\n    @keyframes spin { to { transform: rotate(360deg); } }\n    @media (prefers-reduced-motion: reduce) { i { animation: none; } }\n  </style>\n</head>\n<body>\n  <main role=\"status\" aria-live=\"polite\">\n    <i aria-hidden=\"true\"></i>\n    Carrot Web 업데이트 적용 중\n    <small>Applying update…</small>\n  </main>\n</body>\n</html>\n";

fn render(web: &StaticWeb, manifest: &Value) -> Result<String, Error> {
    let source = fs::read_to_string(web.config.web.join("index.html"))?;
    let html = manifest::inject(&source, manifest)?;
    Ok(html::rewrite(&html, &web.config.web, &web.assets))
}

fn inject_bootstrap(html: &str, bootstrap: &Value) -> Result<String, Error> {
    let payload = manifest::json_utf8(bootstrap)?.replace("</", "<\\/");
    let script = format!(
        "<script id=\"carrotBootstrap\">window.__CARROT_BOOTSTRAP__ = {payload};</script>\n"
    );
    if html.contains("<head>") {
        Ok(html.replacen("<head>", &format!("<head>\n  {script}"), 1))
    } else {
        Ok(format!("{script}{html}"))
    }
}

pub(super) async fn handle<F>(
    web: &Arc<StaticWeb>,
    bootstrap: F,
    head: bool,
) -> Result<Response<Body>, Error>
where
    F: FnOnce() -> Result<Value, Error> + Send + 'static,
{
    let mut loaded = None;
    for delay in [0, 50, 100, 200, 400] {
        if delay != 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        let web = Arc::clone(web);
        let result =
            tokio::task::spawn_blocking(move || render(&web, &web.manifest.load(&web.config.web)?))
                .await;
        match result {
            Ok(Ok(html)) => {
                loaded = Some((html, false));
                break;
            }
            Ok(Err(_)) => {}
            Err(error) => return Err(Error::Source(error.to_string())),
        }
    }
    if loaded.is_none() {
        let web = Arc::clone(web);
        let result = tokio::task::spawn_blocking(move || {
            render(
                &web,
                &Value::object([
                    ("schemaVersion", Value::integer(1)),
                    ("assets", Value::Array(Vec::new())),
                ]),
            )
        })
        .await;
        match result {
            Ok(Ok(html)) => loaded = Some((html, true)),
            Ok(Err(_)) => {}
            Err(error) => return Err(Error::Source(error.to_string())),
        }
    }
    let Some((html, degraded)) = loaded else {
        let mut result = response(
            StatusCode::SERVICE_UNAVAILABLE,
            RECOVERY.as_bytes().to_vec(),
            "text/html; charset=utf-8",
            head,
        );
        result
            .headers_mut()
            .insert(header::RETRY_AFTER, header::HeaderValue::from_static("1"));
        result.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
        result.headers_mut().insert(
            "x-carrot-asset-status",
            header::HeaderValue::from_static("recovering"),
        );
        return Ok(result);
    };
    let bootstrap = tokio::task::spawn_blocking(bootstrap)
        .await
        .map_err(|error| Error::Source(error.to_string()))??;
    let html = inject_bootstrap(&html, &bootstrap)?;
    let mut result = response(
        StatusCode::OK,
        html.into_bytes(),
        "text/html; charset=utf-8",
        head,
    );
    result.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    result
        .headers_mut()
        .insert(header::PRAGMA, header::HeaderValue::from_static("no-cache"));
    result
        .headers_mut()
        .insert(header::EXPIRES, header::HeaderValue::from_static("0"));
    result.headers_mut().insert(
        "x-carrot-asset-status",
        header::HeaderValue::from_static(if degraded { "degraded" } else { "ready" }),
    );
    Ok(result)
}
