pub use crate::http_request::read_json;
use crate::http_response::{error_response, json_response, text};
pub use crate::http_server::serve;
pub use crate::request_body::{decode_request, DecodedBody as RequestBody};
use crate::{
    config::Config,
    param_changes::{History, Paths},
    params::Backend,
    settings::SettingsCache,
    static_web::StaticWeb,
    Error, Value,
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
    pub history: History,
    live_snapshot: Mutex<Option<Value>>,
    settings: Mutex<SettingsCache>,
    pub static_web: Arc<StaticWeb>,
    pub intro: Arc<crate::intro::Intro>,
    pub cars: Arc<crate::cars::Cars>,
    pub mapbox_online: Arc<crate::mapbox_tokens::Online>,
    pub ssh_online: Option<Arc<crate::ssh_keys::Online>>,
    pub ssh_timestamp: Option<i64>,
    pub egpu_model: Arc<crate::egpu_model::ModelFiles>,
    pub popular_values: Arc<crate::popular_values::Service>,
    pub xiaoge_online: Arc<crate::xiaoge::Online>,
}

impl Application {
    pub fn new(config: Config, params: Backend) -> Arc<Self> {
        Arc::new(Self {
            static_web: StaticWeb::new(config.clone()),
            intro: crate::intro::Intro::new(config.clone()),
            cars: crate::cars::Cars::original(),
            mapbox_online: Arc::new(crate::mapbox_tokens::Online::default()),
            ssh_online: Some(Arc::new(crate::ssh_keys::Online::default())),
            ssh_timestamp: None,
            egpu_model: Arc::new(crate::egpu_model::ModelFiles::original(&config.repository)),
            popular_values: crate::popular_values::Service::new(true),
            xiaoge_online: Arc::new(crate::xiaoge::Online::default()),
            history: History::new(Paths {
                log: config.state.join("param_changes.jsonl"),
                baseline: config.state.join("fingerprint_baseline.json"),
            }),
            live_snapshot: Mutex::new(None),
            settings: Mutex::new(SettingsCache::new(config.settings.clone())),
            config,
            params: Mutex::new(params),
        })
    }

    pub(crate) fn settings_payload(&self) -> Result<Value, Error> {
        let params = self
            .params
            .lock()
            .map_err(|_| Error::Source("Params lock poisoned".into()))?;
        let catalog = self.catalog(&params)?.for_brand(&params.vehicle_brand())?;
        let payload = catalog.payload(&self.config.settings, params.has_params());
        let mut response = Value::object([("ok", Value::Bool(true))]);
        if let Value::Object(items) = payload {
            if let Value::Object(fields) = &mut response {
                fields.extend(items);
            }
        }
        Ok(response)
    }

    pub(crate) fn catalog(&self, params: &Backend) -> Result<crate::settings::Catalog, Error> {
        let mut cache = self
            .settings
            .lock()
            .map_err(|_| Error::Source("settings lock poisoned".into()))?;
        cache.load(params.maximum_gap_levels())
    }

    pub fn update_live_snapshot(&self, snapshot: Value) -> Result<(), Error> {
        *self
            .live_snapshot
            .lock()
            .map_err(|_| Error::Source("live snapshot lock poisoned".into()))? = Some(snapshot);
        Ok(())
    }

    pub fn drive_engaged(&self) -> bool {
        self.live_snapshot
            .lock()
            .ok()
            .and_then(|snapshot| {
                snapshot.as_ref().map(|snapshot| {
                    let services = snapshot.get("services");
                    ["selfdriveState", "controlsState"]
                        .into_iter()
                        .any(|name| services.get(name).get("enabled").truth())
                })
            })
            .unwrap_or(false)
    }
}

pub(crate) async fn route(
    mut request: Request<Incoming>,
    app: Arc<Application>,
    sound: crate::web_sound_http::Sessions,
) -> Result<Response<Body>, Infallible> {
    let context = crate::request_body::DecodeContext::default();
    request.extensions_mut().insert(context.clone());
    let head = request.method() == hyper::Method::HEAD;
    let mut request = decode_request(request);
    let prefetched =
        std::future::poll_fn(|cx| std::task::Poll::Ready(request.body_mut().prefetch(cx))).await;
    let mut response = match prefetched {
        Ok(()) => {
            if let Some(expectation) = request
                .extensions()
                .get::<hyper::ext::RawConditionalHeaders>()
                .and_then(|headers| headers.get(&header::EXPECT))
                .or_else(|| request.headers().get(header::EXPECT))
                .filter(|value| {
                    request.version() == hyper::Version::HTTP_11 && !value.as_bytes().is_empty()
                })
            {
                if expectation.as_bytes().eq_ignore_ascii_case(b"100-continue") {
                    match request
                        .extensions()
                        .get::<hyper::ext::ContinueSignal>()
                        .map(hyper::ext::ContinueSignal::send)
                        .transpose()
                    {
                        Ok(_) => dispatch(request, app, sound).await?,
                        Err(error) => error_response(error.to_string(), head),
                    }
                } else {
                    let expectation = String::from_utf8_lossy(expectation.as_bytes());
                    text(
                        StatusCode::EXPECTATION_FAILED,
                        &format!("Unknown Expect: {expectation}"),
                        head,
                    )
                }
            } else {
                dispatch(request, app, sound).await?
            }
        }
        Err(failure) => {
            let error = Error::Request(failure);
            crate::http_response::parser_response(&error, head)
                .unwrap_or_else(|| error_response(error.to_string(), head))
        }
    };
    if context.requires_close() {
        response
            .extensions_mut()
            .insert(hyper::ext::CloseAfterResponse);
    }
    Ok(response)
}

async fn dispatch(
    request: Request<RequestBody>,
    app: Arc<Application>,
    sound: crate::web_sound_http::Sessions,
) -> Result<Response<Body>, Infallible> {
    let head = request.method() == hyper::Method::HEAD;
    let path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .into_owned();
    if path == "/ws/web_sound" {
        return Ok(crate::web_sound_http::handle(request, &app, &sound));
    }
    if path == "/api/cars" {
        return Ok(crate::cars::handle(&request, Arc::clone(&app.cars)).await);
    }
    if crate::egpu_model::matches(&path) {
        let files = Arc::clone(&app.egpu_model);
        return Ok(crate::egpu_model::handle(&request, app, files, &path).await);
    }
    if crate::popular_values::matches(&path) {
        let service = Arc::clone(&app.popular_values);
        return Ok(crate::popular_values::handle(request, app, service).await);
    }
    if crate::xiaoge::matches(&path) {
        let online = Arc::clone(&app.xiaoge_online);
        return Ok(crate::xiaoge::handle(request, &app.config.repository, online).await);
    }
    if path == "/api/ssh_keys" {
        let online = app.ssh_online.clone();
        let timestamp = app.ssh_timestamp;
        return Ok(crate::ssh_keys::handle(request, app, online, timestamp).await);
    }
    if crate::mapbox_tokens::matches(&path, request.method()) {
        let online = Arc::clone(&app.mapbox_online);
        return Ok(crate::mapbox_tokens::handle(request, app, online).await);
    }
    if path == "/download/params_backup.json" {
        return Ok(crate::restore_http::download(&request, &app.config.params_backup).await);
    }
    if crate::restore_http::matches(&path, request.method()) {
        return Ok(crate::restore_http::handle(request, app, &path).await);
    }
    if crate::history_http::matches(&path, request.method()) {
        return Ok(crate::history_http::handle(request, app, &path).await);
    }
    if crate::profiles_http::matches(&path, request.method()) {
        return Ok(crate::profiles_http::handle(request, app, &path).await);
    }
    if let Some(route) = crate::intro::Route::from_path(&path) {
        if matches!(route, crate::intro::Route::State) || request.method() == hyper::Method::POST {
            return Ok(crate::intro::handle(
                request,
                Arc::clone(&app),
                Arc::clone(&app.intro),
                route,
            )
            .await);
        }
        if !matches!(request.method(), &hyper::Method::GET | &hyper::Method::HEAD) {
            let mut response = text(
                StatusCode::METHOD_NOT_ALLOWED,
                "405: Method Not Allowed",
                head,
            );
            response.headers_mut().insert(
                header::ALLOW,
                header::HeaderValue::from_static("GET,HEAD,POST"),
            );
            return Ok(response);
        }
    }
    if path == "/api/params_bulk"
        || (path == "/api/param_set"
            && !matches!(request.method(), &hyper::Method::GET | &hyper::Method::HEAD))
    {
        return Ok(crate::params_http::handle(request, app, &path).await);
    }
    if let Some(kind) = crate::state_preferences::Preference::from_path(&path) {
        return Ok(crate::state_http::handle(request, app, kind).await);
    }
    if path == "/api/web_settings" {
        return Ok(crate::web_settings_http::handle(request, app).await);
    }
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
    let application = Arc::clone(&app);
    Ok(app
        .static_web
        .handle_with_bootstrap(&request, move || crate::bootstrap::payload(&application))
        .await)
}
