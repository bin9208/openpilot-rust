pub use crate::http_request::read_json;
use crate::http_response::{error_response, text};
use crate::http_routes::dispatch;
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
    pub bluetooth_http: Arc<crate::bluetooth_http::Service>,
    pub screenrecord: Arc<crate::screenrecord::Screenrecord>,
    pub dashcam: Arc<crate::dashcam::Service>,
    pub dashcam_metadata: Arc<crate::dashcam::MetadataFiles>,
    pub dashcam_media: Arc<crate::dashcam::Media>,
    pub dashcam_uploads: Arc<crate::dashcam::Uploads>,
    pub dashcam_upload_health: Arc<crate::dashcam::UploadHealth>,
    pub git_status: Option<Arc<crate::git_status::Service>>,
    pub git_state: crate::git_state::Store,
    pub heartbeat: Arc<crate::heartbeat::Service>,
    pub heartbeat_params: Option<openpilot_params::Params>,
}

impl Application {
    pub fn new(config: Config, params: Backend) -> Arc<Self> {
        Arc::new(Self::initialize(config, params, None))
    }

    pub fn with_git_status(
        config: Config,
        params: Backend,
        service: Arc<crate::git_status::Service>,
    ) -> Arc<Self> {
        Arc::new(Self::initialize(config, params, Some(service)))
    }

    pub fn for_runtime(
        config: Config,
        params: Backend,
        git_status: Arc<crate::git_status::Service>,
    ) -> Arc<Self> {
        let heartbeat_params = params.native_params().cloned();
        let mut application = Self::initialize(config, params, Some(git_status));
        application.heartbeat_params = heartbeat_params;
        Arc::new(application)
    }

    fn initialize(
        config: Config,
        params: Backend,
        git_status: Option<Arc<crate::git_status::Service>>,
    ) -> Self {
        let dashcam = crate::dashcam::Service::original(&config);
        let dashcam_metadata = crate::dashcam::MetadataFiles::original(Arc::clone(&dashcam));
        let dashcam_media = crate::dashcam::Media::original(&dashcam, &config);
        let dashcam_uploads = crate::dashcam::Uploads::original(&dashcam);
        let dashcam_upload_health =
            crate::dashcam::UploadHealth::original(&config, params.native_params().cloned());
        Self {
            static_web: StaticWeb::new(config.clone()),
            intro: crate::intro::Intro::new(config.clone()),
            cars: crate::cars::Cars::original(),
            mapbox_online: Arc::new(crate::mapbox_tokens::Online::default()),
            ssh_online: Some(Arc::new(crate::ssh_keys::Online::default())),
            ssh_timestamp: None,
            egpu_model: Arc::new(crate::egpu_model::ModelFiles::original(&config.repository)),
            popular_values: crate::popular_values::Service::new(true),
            xiaoge_online: Arc::new(crate::xiaoge::Online::default()),
            bluetooth_http: crate::bluetooth_http::Service::original(),
            screenrecord: crate::screenrecord::Screenrecord::original(&config),
            dashcam,
            dashcam_metadata,
            dashcam_media,
            dashcam_uploads,
            dashcam_upload_health,
            git_status,
            git_state: crate::git_state::Store::new(config.state.clone()),
            heartbeat: crate::heartbeat::Service::new(),
            heartbeat_params: None,
            history: History::new(Paths {
                log: config.state.join("param_changes.jsonl"),
                baseline: config.state.join("fingerprint_baseline.json"),
            }),
            live_snapshot: Mutex::new(None),
            settings: Mutex::new(SettingsCache::new(config.settings.clone())),
            config,
            params: Mutex::new(params),
        }
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
