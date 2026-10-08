pub use crate::http_request::read_json;
use crate::http_response::{error_response, json_response, response, text};
pub use crate::http_server::serve;
use crate::{
    config::Config, params::Backend, settings::SettingsCache, static_assets, Error, Value,
};
use bytes::Bytes;
use http_body_util::Full;
use hyper::{body::Incoming, header, Request, Response, StatusCode};
use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
};

pub type Body = Full<Bytes>;

pub struct Application {
    pub config: Config,
    pub params: Mutex<Backend>,
    settings: Mutex<SettingsCache>,
    assets: static_assets::Assets,
}

impl Application {
    pub fn new(config: Config, params: Backend) -> Arc<Self> {
        Arc::new(Self {
            settings: Mutex::new(SettingsCache::new(config.settings.clone())),
            config,
            params: Mutex::new(params),
            assets: static_assets::Assets::default(),
        })
    }

    pub(crate) fn settings_payload(&self) -> Result<Value, Error> {
        let params = self
            .params
            .lock()
            .map_err(|_| Error::Source("Params lock poisoned".into()))?;
        let mut cache = self
            .settings
            .lock()
            .map_err(|_| Error::Source("settings lock poisoned".into()))?;
        let catalog = cache
            .load(params.maximum_gap_levels())?
            .for_brand(&params.vehicle_brand())?;
        let payload = catalog.payload(&cache.path, params.has_params());
        let mut response = Value::object([("ok", Value::Bool(true))]);
        if let Value::Object(items) = payload {
            if let Value::Object(fields) = &mut response {
                fields.extend(items);
            }
        }
        Ok(response)
    }
}

pub(crate) async fn route(
    request: Request<Incoming>,
    app: Arc<Application>,
) -> Result<Response<Body>, Infallible> {
    let head = request.method() == hyper::Method::HEAD;
    let path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .into_owned();
    if path == "/api/settings" {
        if request.method() != hyper::Method::GET && !head {
            let mut result = text(
                StatusCode::METHOD_NOT_ALLOWED,
                "405: Method Not Allowed",
                head,
            );
            result
                .headers_mut()
                .insert(header::ALLOW, header::HeaderValue::from_static("GET,HEAD"));
            return Ok(result);
        }
        if !app.config.settings.exists() {
            let payload = Value::object([
                ("ok", Value::Bool(false)),
                (
                    "error",
                    Value::text(&format!(
                        "settings file not found: {}",
                        app.config.settings.display()
                    )),
                ),
            ]);
            return Ok(
                json_response(StatusCode::NOT_FOUND, payload, head, "").unwrap_or_else(|error| {
                    text(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string(), head)
                }),
            );
        }
        let coding = request
            .headers()
            .get(header::ACCEPT_ENCODING)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let result = tokio::task::spawn_blocking(move || app.settings_payload()).await;
        let payload = match result {
            Ok(Ok(payload)) => payload,
            Ok(Err(error)) => return Ok(error_response(error.to_string(), head)),
            Err(error) => return Ok(error_response(error.to_string(), head)),
        };
        return Ok(
            match json_response(StatusCode::OK, payload, head, &coding) {
                Ok(mut response) => {
                    response.headers_mut().insert(
                        header::CACHE_CONTROL,
                        header::HeaderValue::from_static("no-store"),
                    );
                    response
                }
                Err(error) => error_response(error.to_string(), head),
            },
        );
    }
    if request.method() != hyper::Method::GET && !head {
        let mut result = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        result
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static("GET,HEAD"));
        return Ok(result);
    }
    let (root, asset) = if let Some(path) = path.strip_prefix("/shared-assets/") {
        (&app.config.shared_assets, path)
    } else if let Some(path) = path.strip_prefix("/training/") {
        (&app.config.training_assets, path)
    } else if let Some(path) = path.strip_prefix("/sound-assets/") {
        (&app.config.shared_assets, path)
    } else {
        (&app.config.web, path.as_str())
    };
    let Some(file) = static_assets::resolve(root, asset) else {
        if static_assets::missing_inside(root, asset) {
            let mut result = response(
                StatusCode::NOT_FOUND,
                Vec::new(),
                "application/octet-stream",
                head,
            );
            result.headers_mut().remove(header::CONTENT_LENGTH);
            result.headers_mut().insert(
                header::TRANSFER_ENCODING,
                header::HeaderValue::from_static("chunked"),
            );
            return Ok(result);
        }
        return Ok(text(StatusCode::NOT_FOUND, "404: Not Found", head));
    };
    let immutable = app
        .assets
        .immutable(&app.config.web, &path, request.uri().query());
    if root == &app.config.web
        && ["/js/", "/css/", "/assets/"]
            .iter()
            .any(|prefix| path.starts_with(prefix))
    {
        let _ = static_assets::refresh_gzip(root, asset);
    }
    let selected = if request
        .headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|header| header.to_str().ok())
        .is_some_and(|header| header.to_lowercase().contains("gzip"))
    {
        let gzip = std::path::PathBuf::from(format!("{}.gz", file.display()));
        if gzip.is_file() {
            gzip
        } else {
            file.clone()
        }
    } else {
        file.clone()
    };
    let result = (|| -> Result<Response<Body>, Error> {
        let metadata = std::fs::metadata(&selected)?;
        let etag = static_assets::etag(&metadata)?;
        let mut result = if request
            .headers()
            .get(header::IF_NONE_MATCH)
            .and_then(|header| header.to_str().ok())
            .is_some_and(|header| {
                header.split(',').any(|value| {
                    value.trim().trim_start_matches("W/") == etag || value.trim() == "*"
                })
            }) {
            let mut result = response(
                StatusCode::NOT_MODIFIED,
                Vec::new(),
                static_assets::content_type(&file),
                head,
            );
            result.headers_mut().remove(header::CONTENT_LENGTH);
            result
        } else {
            response(
                StatusCode::OK,
                std::fs::read(&selected)?,
                static_assets::content_type(&file),
                head,
            )
        };
        result.headers_mut().insert(
            header::ETAG,
            etag.parse()
                .map_err(|_| Error::Source("invalid asset ETag".into()))?,
        );
        result.headers_mut().insert(
            header::LAST_MODIFIED,
            static_assets::last_modified(&metadata)?
                .parse()
                .map_err(|_| Error::Source("invalid asset timestamp".into()))?,
        );
        result.headers_mut().insert(
            header::ACCEPT_RANGES,
            header::HeaderValue::from_static("bytes"),
        );
        if selected != file {
            result.headers_mut().insert(
                header::CONTENT_ENCODING,
                header::HeaderValue::from_static("gzip"),
            );
            result.headers_mut().insert(
                header::VARY,
                header::HeaderValue::from_static("Accept-Encoding"),
            );
        }
        if immutable {
            result.headers_mut().insert(
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
        }
        Ok(result)
    })();
    Ok(result.unwrap_or_else(|_| text(StatusCode::NOT_FOUND, "404: Not Found", head)))
}
