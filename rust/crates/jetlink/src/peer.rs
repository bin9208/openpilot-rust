//! Descriptive source peer allowlist, never authentication or activation proof.
use serde_json::Value;
use std::sync::LazyLock;
fn matches(pattern: &regex::Regex, text: Option<&str>) -> bool {
    let Some(text) = text else {
        return false;
    };
    pattern.is_match(text)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mac,
    Android,
    Ios,
}
pub fn classify(peer: &Value, path: &str) -> Option<Kind> {
    static MAC: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"(?i)\A(?:coreml|ane(?:-whole)?)-Apple[ _-]M[1-9][0-9]*(?:[ _-](?:Pro|Max|Ultra))?\z",
        )
        .expect("constant peer pattern")
    });
    static IOS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?i)\A(?:coreml|ane(?:-whole)?)-Apple[ _-][AM][1-9][0-9]*(?:[ _-](?:Pro|Max|Ultra))?\z").expect("constant peer pattern")
    });
    static ANDROID: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?i)\A(?:htp(?:-whole)?|gpu|npu)-[A-Za-z0-9_.-]+\z")
            .expect("constant peer pattern")
    });
    let version = peer["protocol"].as_u64()?;
    let host = &peer["carrot_host"];
    let backend = peer["backend"].as_str()?;
    let device = peer["device"].as_str();
    if path == "usb"
        && [2, 3].contains(&version)
        && (host.is_null() || host == "mac")
        && backend == "ort"
        && matches(&MAC, device)
    {
        return Some(Kind::Mac);
    }
    if version != 3 || !host.is_null() {
        return None;
    }
    if path == "usb" && matches!(backend, "ort" | "litert") && matches(&ANDROID, device) {
        Some(Kind::Android)
    } else if path == "tcp" && backend == "ort" && matches(&IOS, device) {
        Some(Kind::Ios)
    } else {
        None
    }
}
