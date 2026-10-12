use super::Service;
use crate::tools::runner::Failure;
use crate::{
    http::{Body, RequestBody},
    http_response::{json_response, text},
    Value,
};
use hyper::{header, Request, Response, StatusCode};
use std::sync::Arc;

fn response(status: StatusCode, value: Value) -> Response<Body> {
    json_response(status, value, false, "").unwrap_or_else(|_| internal())
}
fn invalid(message: &str) -> Response<Body> {
    response(
        StatusCode::BAD_REQUEST,
        Value::object([("ok", Value::Bool(false)), ("error", Value::text(message))]),
    )
}
fn internal() -> Response<Body> {
    let mut response = text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        false,
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
pub(super) async fn run(request: Request<RequestBody>, service: Arc<Service>) -> Response<Body> {
    let Ok(admission) = service.admit_command() else {
        return internal();
    };
    let Ok(body) = crate::http::read_json(request).await else {
        return invalid("invalid json");
    };
    if !matches!(body, Value::Object(_)) {
        return internal();
    }
    let raw_name = if body.get("command").truth() {
        body.get("command").py_string()
    } else {
        Ok(Value::text(""))
    };
    let Ok(Value::Text(points)) = raw_name else {
        return internal();
    };
    let points = crate::state::trim(&points);
    let name_points: Vec<_> = points
        .iter()
        .flat_map(|point| match char::from_u32(*point) {
            Some(c) => c.to_lowercase().map(u32::from).collect::<Vec<_>>(),
            None => vec![*point],
        })
        .collect();
    let raw_args = if body.has("args") {
        body.get("args").clone()
    } else {
        Value::Array(Vec::new())
    };
    let Value::Array(args) = raw_args else {
        return invalid("args must be a list");
    };
    let args = args
        .iter()
        .map(Value::py_string)
        .collect::<Result<Vec<_>, _>>();
    let Ok(args) = args else {
        return internal();
    };
    if name_points.is_empty()
        || args.len() > 32
        || args
            .iter()
            .any(|arg| matches!(arg, Value::Text(points) if points.len() > 256))
    {
        return invalid("invalid command arguments");
    }
    let known = crate::terminal_commands::registry::COMMANDS
        .iter()
        .find(|command| {
            name_points
                .iter()
                .copied()
                .eq(command.name.chars().map(u32::from))
        });
    let Some(command) = known else {
        let mut points: Vec<_> = "unknown command: ".chars().map(u32::from).collect();
        points.extend(name_points);
        return response(
            StatusCode::NOT_FOUND,
            Value::object([("ok", Value::Bool(false)), ("error", Value::Text(points))]),
        );
    };
    let args = args
        .iter()
        .map(Value::string)
        .collect::<Result<Vec<_>, _>>();
    let Ok(args) = args else {
        return internal();
    };
    let Some(program) = service.config.cli.to_str() else {
        return internal();
    };
    let mut argv = vec![program.to_owned(), command.name.into()];
    argv.extend(args);
    let result = service.capture(argv, admission).await;
    let output = match result {
        Ok(output) => output,
        Err(Failure::Timeout) => {
            return response(
                StatusCode::GATEWAY_TIMEOUT,
                Value::object([
                    ("ok", Value::Bool(false)),
                    ("error", Value::text("command timed out")),
                ]),
            )
        }
        Err(error) => {
            eprintln!("Terminal CLI launch/capture: {error}");
            return internal();
        }
    };
    use std::os::unix::process::ExitStatusExt;
    let rc = output
        .status
        .code()
        .unwrap_or_else(|| -output.status.signal().unwrap_or(0));
    let stripped = |bytes: &[u8]| {
        let text = String::from_utf8_lossy(bytes);
        text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
            .to_owned()
    };
    response(
        if rc == 0 {
            StatusCode::OK
        } else {
            StatusCode::BAD_REQUEST
        },
        Value::object([
            ("ok", Value::Bool(rc == 0)),
            ("rc", Value::integer(rc)),
            ("out", Value::text(&stripped(&output.stdout))),
            ("error", Value::text(&stripped(&output.stderr))),
        ]),
    )
}
