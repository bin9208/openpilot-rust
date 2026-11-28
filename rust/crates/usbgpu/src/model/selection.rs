//! Current source selection is independent of downloaded historical artifacts.
use super::{Manifest, Paths};
use serde_json::Value;
fn allowed(active: &Manifest, url: &str, state: Option<&Value>, failed: bool) -> bool {
    if failed {
        return false;
    }
    let pinned = crate::library::current().catalog_manifest(url).or_else(|| {
        (url == super::DEFAULT_MANIFEST_URL).then(crate::model_delivery::pinned_manifest)
    });
    if let Some(selected) = pinned {
        active.sha256 == selected.sha256
            && active.size == selected.size
            && active.filename == selected.filename
    } else {
        state
            .and_then(|value| value.get("manifest_url"))
            .and_then(Value::as_str)
            == Some(url)
    }
}
pub fn selected_manifest(paths: &Paths) -> Option<Manifest> {
    let failed =
        std::env::var_os("CARROT_BIG_MODEL_STARTUP_FAILED").is_some_and(|value| value == "1");
    if failed {
        return None;
    }
    let active = super::active_manifest(paths)?;
    let url = match std::env::var("CARROT_BIG_MODEL_MANIFEST") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => crate::library::current()
            .selected_manifest_url()
            .unwrap_or_else(|| super::DEFAULT_MANIFEST_URL.into()),
        Err(_) => return None,
    };
    allowed(
        &active,
        &url,
        super::json(&paths.cache.join("state.json")).as_ref(),
        failed,
    )
    .then_some(active)
}
