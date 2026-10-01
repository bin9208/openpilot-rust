use crate::{arguments, Error};
use openpilot_logmessaged::{JsonValue, JsonView};
use serde_json::json;

pub struct Fault {
    pub code: i32,
    pub kind: Option<String>,
    pub message: String,
    pub args: Option<Box<serde_json::Value>>,
}
impl Fault {
    pub fn standard(code: i32) -> Self {
        Self {
            code,
            kind: None,
            message: String::new(),
            args: None,
        }
    }
    pub fn server(kind: &str, message: impl Into<String>) -> Self {
        Self {
            code: -32000,
            kind: Some(kind.into()),
            message: message.into(),
            args: None,
        }
    }
    pub fn params(message: String) -> Self {
        Self {
            code: -32602,
            kind: Some("TypeError".into()),
            message,
            args: None,
        }
    }
    pub fn io(error: std::io::Error, path: &std::path::Path) -> Self {
        let kind = match error.kind() {
            std::io::ErrorKind::NotFound => "FileNotFoundError",
            std::io::ErrorKind::PermissionDenied => "PermissionError",
            std::io::ErrorKind::NotADirectory => "NotADirectoryError",
            std::io::ErrorKind::IsADirectory => "IsADirectoryError",
            _ => "OSError",
        };
        let number = error.raw_os_error().unwrap_or(5);
        let text = error
            .to_string()
            .split(" (os error ")
            .next()
            .unwrap_or("")
            .to_owned();
        Self {
            code: -32000,
            kind: Some(kind.into()),
            message: format!("[Errno {number}] {text}: '{}'", path.display()),
            args: Some(Box::new(json!([number, text]))),
        }
    }
    pub fn json(&self) -> String {
        let message = match self.code {
            -32700 => "Parse error",
            -32600 => "Invalid Request",
            -32601 => "Method not found",
            -32602 => "Invalid params",
            -32603 => "Internal error",
            _ => "Server error",
        };
        let mut error = json!({"code":self.code,"message":message});
        if let Some(kind) = &self.kind {
            let args = if self.message.is_empty() {
                Vec::<String>::new()
            } else {
                vec![self.message.clone()]
            };
            error["data"] = json!({"type":kind,"args":self.args.as_deref().cloned().unwrap_or_else(||json!(args)),"message":self.message});
        }
        error.to_string()
    }
}
impl From<Error> for Fault {
    fn from(error: Error) -> Self {
        match error {
            Error::Timeout => Self::server("TimeoutError", ""),
            Error::Contract("Object of type bytes is not JSON serializable") => Self {
                code: 0,
                kind: None,
                message: "Object of type bytes is not JSON serializable".into(),
                args: None,
            },
            Error::Contract(message) => Self::server("Exception", message),
            Error::Io(error) => Self::server("OSError", error.to_string()),
            error => Self::server("Exception", error.to_string()),
        }
    }
}
impl From<openpilot_logmessaged::JsonError> for Fault {
    fn from(error: openpilot_logmessaged::JsonError) -> Self {
        Self::from(Error::from(error))
    }
}
pub enum Route {
    Reply(String),
    LogResponse(String),
}

pub fn route(
    text: &str,
    binary: bool,
    mut invoke: impl FnMut(&str, Vec<JsonValue>) -> Result<JsonValue, Fault>,
) -> Route {
    if binary {
        return Route::Reply(
            json!({"error":"a bytes-like object is required, not 'str'"}).to_string(),
        );
    }
    if !text.contains("method") {
        return if text.contains("id") && (text.contains("result") || text.contains("error")) {
            Route::LogResponse(text.into())
        } else {
            Route::Reply(json!({"error":"not a valid request or response"}).to_string())
        };
    }
    let result = match JsonValue::parse(text) {
        Err(_) => error_response(-32700),
        Ok(value) => match handle(value, &mut invoke) {
            Ok(Some(response)) => response,
            Ok(None) => json!({"error":"'NoneType' object has no attribute 'json'"}).to_string(),
            Err(message) => json!({"error":message}).to_string(),
        },
    };
    Route::Reply(result)
}

fn handle(
    value: JsonValue,
    invoke: &mut impl FnMut(&str, Vec<JsonValue>) -> Result<JsonValue, Fault>,
) -> Result<Option<String>, String> {
    let batch = matches!(value.view(), JsonView::Array(_));
    let version1 = value.is_object() && value.get("jsonrpc").is_none();
    let values = match value.view() {
        JsonView::Array(values) => values,
        _ => vec![value],
    };
    if values.is_empty() {
        return Ok(Some(error_response(-32600)));
    }
    for value in &values {
        if !valid(value, version1) {
            return Ok(Some(error_response(-32600)));
        }
    }
    let mut responses = Vec::new();
    for value in values {
        let method_value = value
            .get("method")
            .ok_or_else(|| "Method should be string".to_owned())?;
        if !matches!(method_value.view(), JsonView::Text(_)) {
            return Err("Method should be string".into());
        }
        let method = method_value
            .to_utf8()
            .unwrap_or_else(|| "unencodable-method-not-in-dispatcher".into());
        if version1 {
            let params = value
                .get("params")
                .ok_or_else(|| "missing params".to_owned())?;
            if !matches!(params.view(), JsonView::Array(_)) {
                let text = openpilot_runtime_version::python_str(&params)
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .filter_map(char::from_u32)
                    .collect::<String>();
                return Err(format!("Incorrect params {text}"));
            }
        }
        let result =
            arguments::bind(&method, value.get("params")).and_then(|args| invoke(&method, args));
        let id = value.get("id");
        if version1
            && id
                .as_ref()
                .is_some_and(|value| matches!(value.view(), JsonView::Null))
        {
            return Err("id could not be null for JSON-RPC1.0 Response".into());
        }
        if id.is_none() {
            continue;
        }
        let id = id
            .ok_or_else(|| "missing id".to_owned())?
            .to_json()
            .map_err(|error| error.to_string())?;
        let result = match result {
            Ok(result) => format!(
                "\"result\":{}",
                result.to_json().map_err(|error| error.to_string())?
            ),
            Err(error) if error.code == 0 => return Err(error.message),
            Err(error) => format!("\"error\":{}", error.json()),
        };
        responses.push(format!(
            "{{{result},\"id\":{id}{}}}",
            if version1 { "" } else { ",\"jsonrpc\":\"2.0\"" }
        ));
    }
    Ok(if responses.is_empty() {
        None
    } else if batch {
        Some(format!("[{}]", responses.join(",")))
    } else {
        responses.into_iter().next()
    })
}
fn valid(value: &JsonValue, version1: bool) -> bool {
    let JsonView::Object(fields) = value.view() else {
        return false;
    };
    if fields.iter().any(|(key, _)| {
        !["jsonrpc", "method", "params", "id"]
            .iter()
            .any(|name| key.iter().copied().eq(name.chars().map(u32::from)))
    }) {
        return false;
    }
    let Some(method) = value.get("method") else {
        return false;
    };
    if version1 {
        return value.get("id").is_some() && value.get("params").is_some();
    }
    if value.get("jsonrpc").is_none()
        || !matches!(method.view(), JsonView::Text(_))
        || method
            .to_utf8()
            .is_some_and(|method| method.starts_with("rpc."))
    {
        return false;
    }
    if value.get("params").is_some_and(|params| {
        !matches!(
            params.view(),
            JsonView::Null | JsonView::Array(_) | JsonView::Object(_)
        )
    }) {
        return false;
    }
    !value.get("id").is_some_and(|id| {
        !matches!(
            id.view(),
            JsonView::Null | JsonView::Text(_) | JsonView::Integer(_) | JsonView::Bool(_)
        )
    })
}
fn error_response(code: i32) -> String {
    format!(
        "{{\"error\":{},\"id\":null,\"jsonrpc\":\"2.0\"}}",
        Fault::standard(code).json()
    )
}
