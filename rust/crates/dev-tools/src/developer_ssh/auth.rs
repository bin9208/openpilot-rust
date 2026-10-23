//! Read-only auth_config.get_token subset; original repository MIT license applies.
use openpilot_log_format::JsonValue;
use std::path::{Path, PathBuf};

pub fn config_root(home: &Path, prefix: &str, tici: bool) -> PathBuf {
    if tici {
        PathBuf::from("/tmp/.comma")
    } else {
        home.join(format!(".comma{prefix}"))
    }
}
/// Caller supplies a root so source tests use synthetic private directories only.
/// Missing file, invalid UTF-8/JSON, missing key and wrong top-level type all become None.
/// No truthiness/type validation is added to the returned access_token.
pub fn get_token_at(root: &Path) -> JsonValue {
    std::fs::read_to_string(root.join("auth.json"))
        .ok()
        .and_then(|text| JsonValue::parse(&text).ok())
        .and_then(|auth| auth.get("access_token"))
        .unwrap_or_else(|| JsonValue::parse("null").expect("JSON null"))
}
