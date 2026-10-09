use crate::{
    http::{Application, Body, RequestBody},
    http_response::{error_response, json_response, text},
    Value,
};
use hyper::{header, Request, Response, StatusCode};
use std::{convert::Infallible, sync::Arc};

pub(crate) async fn dispatch(
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
    if crate::system::http::matches(&path) {
        return Ok(crate::system::http::handle(request, app, path).await);
    }
    if crate::live::http::matches(&path) {
        return Ok(
            crate::live::http::handle(request, app.live.clone(), app.live_error.as_deref()).await,
        );
    }
    if crate::web_navi::http::matches(&path) {
        return Ok(crate::web_navi::http::handle(request, app.web_navi.clone()).await);
    }
    if path == "/api/cars" {
        return Ok(crate::cars::handle(&request, Arc::clone(&app.cars)).await);
    }
    if path == "/api/tools/git_status" {
        return Ok(match &app.git_status {
            Some(service) => {
                crate::tools_git_status::handle(&request, service, &app.git_state).await
            }
            None => crate::tools_git_status::unavailable(head),
        });
    }
    if path == "/api/heartbeat_status" {
        return Ok(crate::heartbeat::handle(&request, &app.heartbeat));
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
    if crate::bluetooth_http::matches(&path) {
        let service = Arc::clone(&app.bluetooth_http);
        return Ok(crate::bluetooth_http::handle(request, service).await);
    }
    if crate::screenrecord::matches(request.uri().path()) {
        let service = Arc::clone(&app.screenrecord);
        return Ok(crate::screenrecord::handle(request, service).await);
    }
    if crate::dashcam::matches(request.uri().path()) {
        return Ok(crate::dashcam::handle(request, Arc::clone(&app.dashcam)).await);
    }
    if crate::dashcam::metadata_matches(request.uri().path()) {
        return Ok(
            crate::dashcam::metadata_handle(request, Arc::clone(&app.dashcam_metadata)).await,
        );
    }
    if crate::dashcam::media_matches(request.uri().path()) {
        return Ok(crate::dashcam::media_handle(request, Arc::clone(&app.dashcam_media)).await);
    }
    if crate::dashcam::report_matches(request.uri().path()) {
        return Ok(crate::dashcam::report_handle(request, Arc::clone(&app.dashcam)).await);
    }
    if crate::dashcam::health_matches(request.uri().path()) {
        return Ok(
            crate::dashcam::health_handle(request, Arc::clone(&app.dashcam_upload_health)).await,
        );
    }
    if crate::dashcam::sync_upload_matches(request.uri().path()) {
        return Ok(crate::dashcam::sync_upload_handle(
            request,
            Arc::clone(&app.dashcam_sync_uploads),
        )
        .await);
    }
    if crate::dashcam::upload_matches(request.uri().path()) {
        return Ok(crate::dashcam::upload_handle(request, Arc::clone(&app.dashcam_uploads)).await);
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
