//! Current source selection is independent of downloaded historical artifacts.
use super::{Manifest, Paths};
use serde_json::Value;
fn allowed(active: &Manifest, url: &str, state: Option<&Value>, failed: bool) -> bool {
    if failed {
        return false;
    }
    if url == super::DEFAULT_MANIFEST_URL {
        let selected = crate::model_delivery::pinned_manifest();
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
        Err(std::env::VarError::NotPresent) => super::DEFAULT_MANIFEST_URL.into(),
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_selection_matches_original_source_catalog_and_failure_cases() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/model_selection_source.json"
        ))
        .unwrap();
        for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let active =
                serde_json::from_value::<Option<Manifest>>(case["active"].clone()).unwrap();
            let actual = active.as_ref().is_some_and(|active| {
                allowed(
                    active,
                    case["url"].as_str().unwrap(),
                    Some(&case["state"]),
                    case["failed"].as_bool().unwrap(),
                )
            });
            assert_eq!(
                actual,
                case["selected"].as_bool().unwrap(),
                "source case {index}"
            );
        }
    }
}
