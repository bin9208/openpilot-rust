//! Model library API and offroad maintenance from carrot's
//! `server/features/egpu_model.py` (`api_library`, `library_maintenance`).
use crate::http::{Body, RequestBody};
use http_body_util::BodyExt;
use hyper::{Method, Request, Response, StatusCode};
use openpilot_usbgpu::library::Library;
use std::path::Path;

pub const ROUTE: &str = "/api/egpu/model/library";

fn monotonic() -> f64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    t.tv_sec as f64 + t.tv_nsec as f64 * 1e-9
}

/// `model_library.status()` for the status payload.
pub fn status(repository: &Path) -> serde_json::Value {
    Library::for_runtime(repository)
        .status(monotonic())
        .unwrap_or_else(|error| serde_json::json!({"error": error.to_string()}))
}

fn reply(code: StatusCode, value: serde_json::Value) -> Response<Body> {
    let mut response = Response::new(Body::from(value.to_string()));
    *response.status_mut() = code;
    response.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

pub async fn handle(request: Request<RequestBody>, repository: &Path) -> Response<Body> {
    if request.method() != Method::POST {
        return reply(
            StatusCode::METHOD_NOT_ALLOWED,
            serde_json::json!({"ok": false, "error": "405"}),
        );
    }
    let Ok(collected) = request.into_body().collect().await else {
        return reply(
            StatusCode::CONFLICT,
            serde_json::json!({"ok": false, "error": "invalid body"}),
        );
    };
    let bytes = collected.to_bytes();
    let repository = repository.to_owned();
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let body: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "invalid model action")?;
        let fields = body.as_object().ok_or("invalid model action")?;
        if fields.len() != 2 || !fields.contains_key("action") || !fields.contains_key("model") {
            return Err("invalid model action".into());
        }
        let library = Library::for_runtime(&repository);
        let key = body["model"].as_str().unwrap_or_default();
        let available = library.available().map_err(|e| e.to_string())?;
        if body["action"] != "select" || !available.contains_key(key) {
            return Err("unknown model action".into());
        }
        library.select(key).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({"ok": true, "library": library.status(monotonic()).map_err(|e| e.to_string())?}))
    })
    .await;
    match result {
        Ok(Ok(value)) => reply(StatusCode::OK, value),
        Ok(Err(error)) => reply(
            StatusCode::CONFLICT,
            serde_json::json!({"ok": false, "error": error}),
        ),
        Err(error) => reply(
            StatusCode::CONFLICT,
            serde_json::json!({"ok": false, "error": error.to_string()}),
        ),
    }
}

/// `library_maintenance`: apply catalog retirements every 30 s offroad.
pub fn start_maintenance(repository: &Path, params: Option<openpilot_params::Params>) {
    let Some(params) = params else {
        return;
    };
    let library = Library::for_runtime(repository);
    let _ = std::thread::Builder::new()
        .name("model-library".into())
        .spawn(move || loop {
            let offroad = || {
                params.get_bool("IsOffroad").unwrap_or(false)
                    && !params.get_bool("IsOnroad").unwrap_or(false)
            };
            library.reconcile_retired(&offroad);
            std::thread::sleep(std::time::Duration::from_secs(30));
        });
}
