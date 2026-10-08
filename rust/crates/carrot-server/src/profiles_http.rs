use crate::{
    http::{read_json, Application, Body},
    http_response::{json_response, text},
    json_fields::fields,
    param_changes::text::stripped,
    param_restore::Restore,
    setting_profiles::{now_iso, Git, ProfileError, ProfileStore},
    settings::Catalog,
    Error, Value,
};
use hyper::{body::Incoming, header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub(crate) fn matches(path: &str, method: &Method) -> bool {
    path == "/api/setting_profiles"
        || (matches!(
            path,
            "/api/setting_profiles/update"
                | "/api/setting_profiles/delete"
                | "/api/setting_profiles/preview"
                | "/api/setting_profiles/apply"
        ) && !matches!(method, &Method::GET | &Method::HEAD))
}
fn failure(head: bool) -> Response<Body> {
    text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    )
}
fn reply(status: StatusCode, value: Value, head: bool) -> Response<Body> {
    json_response(status, value, head, "").unwrap_or_else(|_| failure(head))
}
fn error_reply(error: ProfileError, head: bool) -> Response<Body> {
    let status = if matches!(error, ProfileError::NotFound) {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::BAD_REQUEST
    };
    let mut value = Value::object([
        ("ok", Value::Bool(false)),
        ("error", Value::text(&error.to_string())),
    ]);
    if let Some(code) = error.code() {
        if let Err(error) = crate::json_fields::set(&mut value, "error_code", Value::text(code)) {
            eprintln!("profile error response: {error}");
            return failure(head);
        }
    }
    reply(status, value, head)
}
fn process(app: &Application, path: &str, get: bool, body: &Value) -> Result<Value, ProfileError> {
    if !get {
        fields(body)?;
    }
    let create = path == "/api/setting_profiles" && !get;
    let id = stripped(body.get("id"), true)?;
    if !get && !create && !id.truth() {
        return Err(Error::Source("missing profile id".into()).into());
    }
    let mut params = app
        .params
        .lock()
        .map_err(|_| Error::Source("Params lock poisoned".into()))?;
    let profiles_path = app.config.state.join("setting_profiles.json");
    let raw = crate::state::read(&profiles_path);
    let needs_catalog = (create && stripped(body.get("name"), true)?.truth())
        || matches!(raw.get("profiles"), Value::Array(profiles) if profiles.iter().any(|profile| matches!(profile.get("values"), Value::Object(_))));
    let catalog = if create && !stripped(body.get("name"), true)?.truth() {
        Catalog::from_data(Value::object([("params", Value::Array(Vec::new()))]))?
    } else {
        match app.catalog(&params) {
            Ok(catalog) => catalog,
            Err(error) if needs_catalog => return Err(error.into()),
            Err(_) => Catalog::from_data(Value::object([("params", Value::Array(Vec::new()))]))?,
        }
    };
    let store = ProfileStore::new(&profiles_path, &catalog);
    let mut result = Value::object([("ok", Value::Bool(true))]);
    if get {
        if let Value::Object(values) = store.read()? {
            if let Value::Object(result) = &mut result {
                result.extend(values);
            }
        }
        return Ok(result);
    }
    let mut restored = None;
    let profile = if create {
        Some(store.create_current(
            body.get("name"),
            &Git {
                repository: app.config.repository.clone(),
                program: "git".into(),
            },
            &params,
        )?)
    } else if path.ends_with("/update") {
        Some(store.update(&id, body, &now_iso())?)
    } else if path.ends_with("/delete") {
        store.delete(&id)?;
        None
    } else {
        let values = body.get("values");
        let mut restore = Restore::new(&mut params, &catalog, &app.history);
        if path.ends_with("/preview") {
            crate::json_fields::set(
                &mut result,
                "preview",
                store.preview(&id, values, &restore)?,
            )?;
        } else {
            restored = Some(store.apply(&id, values, &mut restore)?);
        }
        return if let Some(Value::Object(restored)) = restored {
            if let Value::Object(result) = &mut result {
                result.extend(restored);
            }
            Ok(result)
        } else {
            Ok(result)
        };
    };
    if let Some(profile) = profile {
        crate::json_fields::set(&mut result, "profile", profile)?;
    }
    if let Value::Object(values) = store.read()? {
        if let Value::Object(result) = &mut result {
            result.extend(values);
        }
    }
    Ok(result)
}

pub(crate) async fn handle(
    request: Request<Incoming>,
    app: Arc<Application>,
    path: &str,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let get = matches!(request.method(), &Method::GET | &Method::HEAD);
    if !get && request.method() != Method::POST {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static("GET,HEAD,POST"),
        );
        return response;
    }
    let body = if get {
        Value::Object(Vec::new())
    } else {
        read_json(request)
            .await
            .unwrap_or_else(|_| Value::Object(Vec::new()))
    };
    let create = path == "/api/setting_profiles" && !get;
    if !get && !create && !matches!(body, Value::Object(_)) {
        return failure(head);
    }
    let path = path.to_owned();
    match tokio::task::spawn_blocking(move || process(&app, &path, get, &body)).await {
        Ok(Ok(result)) => reply(StatusCode::OK, result, head),
        Ok(Err(error)) if get => {
            eprintln!("profile read: {error}");
            failure(head)
        }
        Ok(Err(error)) => error_reply(error, head),
        Err(error) => {
            eprintln!("profile task: {error}");
            failure(head)
        }
    }
}
