//! Original Tools Git status GET route from features/tools/routes.py (#225).
use crate::{
    git_state::Store,
    git_status::{Service, State, Status},
    http::Body,
    http_response::{json_response, text},
    Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

fn force<T>(request: &Request<T>) -> bool {
    ["force", "refresh"].into_iter().any(|name| {
        url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
            .find(|(key, _)| key == name)
            .is_some_and(|(_, value)| {
                let points: Vec<_> = value.chars().map(u32::from).collect();
                let value: String = crate::state::trim(&points)
                    .iter()
                    .filter_map(|point| char::from_u32(*point))
                    .collect();
                matches!(value.to_lowercase().as_str(), "1" | "true" | "yes")
            })
    })
}

fn payload(status: Status, store: &Store) -> Value {
    let state = match status.state {
        State::Error => "error",
        State::Busy => "busy",
        State::NoUpstream => "no_upstream",
        State::Ok => "ok",
        State::FetchError => "fetch_error",
    };
    let mut fields = vec![
        ("ok", Value::Bool(true)),
        ("available", Value::Bool(status.available)),
        ("state", Value::text(state)),
        ("behind", Value::integer(status.behind)),
        ("ahead", Value::integer(status.ahead)),
        ("branch", Value::text(&status.branch)),
    ];
    if let Some(head) = status.head {
        fields.push(("head", Value::text(&head)));
    }
    if let Some(target) = status.target_head {
        fields.push(("target_head", Value::text(&target)));
    }
    fields.push(("upstream", Value::text(&status.upstream)));
    if let Some(remote) = status.remote {
        fields.push(("remote", Value::text(&remote)));
    }
    if let Some(branch) = status.remote_branch {
        fields.push(("remote_branch", Value::text(&branch)));
    }
    fields.push(("checked_at", Value::integer(status.checked_at)));
    fields.push(("error", Value::text(&status.error)));
    if let Some(error) = status.fetch_error {
        fields.push(("fetch_error", Value::text(&error)));
    }
    fields.push(("auto_update", store.auto_update()));
    Value::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.chars().map(u32::from).collect(), value))
            .collect(),
    )
}

pub async fn handle<T>(
    request: &Request<T>,
    service: &Arc<Service>,
    store: &Store,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    if request.method() != Method::GET && !head {
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
    let result = service.get(force(request)).await;
    match result {
        Ok(status) => json_response(StatusCode::OK, payload(status, store), head, "")
            .unwrap_or_else(|error| {
                eprintln!("Git status response: {error}");
                unavailable(head)
            }),
        Err(error) => {
            eprintln!("Git status request: {error}");
            unavailable(head)
        }
    }
}

pub(crate) fn unavailable(head: bool) -> Response<Body> {
    let mut response = text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    );
    response.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("close"),
    );
    response
        .extensions_mut()
        .insert(hyper::ext::CloseAfterResponse);
    response
}
