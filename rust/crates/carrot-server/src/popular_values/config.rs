use super::{defaults, payload::param_text};
use crate::{params::Backend, Value};
use num_traits::ToPrimitive;

pub(super) const USER_AGENT: &str = "openpilot-carrot-param-value/1";

fn environment(name: &str) -> String {
    super::payload::strip(&std::env::var(name).unwrap_or_default()).to_owned()
}

fn resolved(params: &Backend, environment_name: &str, param_name: &str) -> String {
    let value = environment(environment_name);
    if value.is_empty() {
        param_text(params, param_name)
    } else {
        value
    }
}

pub fn env_float(name: &str, default: f64) -> f64 {
    let text = environment(name);
    if text.is_empty() {
        default
    } else {
        Value::text(&text).float().unwrap_or(default)
    }
}

pub fn env_int(name: &str, default: u64) -> u64 {
    let text = environment(name);
    if text.is_empty() {
        return default;
    }
    match Value::text(&text).int() {
        Ok(value) if value <= 1.into() => 1,
        Ok(value) => value.to_u64().unwrap_or(u64::MAX),
        Err(_) => default,
    }
}

pub fn endpoint(params: &Backend, popular: bool) -> String {
    let (environment_name, param_name, path) = if popular {
        (
            "CARROT_PARAM_VALUE_POPULAR_URL",
            "CarrotParamValuePopularUrl",
            "popular",
        )
    } else {
        (
            "CARROT_PARAM_VALUE_SNAPSHOT_URL",
            "CarrotParamValueSnapshotUrl",
            "snapshot",
        )
    };
    let value = resolved(params, environment_name, param_name);
    if !value.is_empty() {
        return value;
    }
    let base = resolved(params, "CARROT_PARAM_VALUE_URL", "CarrotParamValueUrl");
    let base = if base.is_empty() {
        defaults::decode(
            defaults::DEFAULT_BASE_URL_BYTES,
            defaults::DEFAULT_BASE_URL_KEY,
        )
    } else {
        base
    };
    format!("{}/api/carrot-settings/{path}", base.trim_end_matches('/'))
}

pub fn credentials(params: &Backend) -> (String, String) {
    let id = resolved(params, "CARROT_PARAM_VALUE_CF_ID", "CarrotParamValueCfId");
    let secret = resolved(
        params,
        "CARROT_PARAM_VALUE_CF_SECRET",
        "CarrotParamValueCfSecret",
    );
    (
        if id.is_empty() {
            defaults::decode(
                defaults::DEFAULT_CF_ACCESS_ID_BYTES,
                defaults::DEFAULT_CF_ACCESS_ID_KEY,
            )
        } else {
            id
        },
        if secret.is_empty() {
            defaults::decode(
                defaults::DEFAULT_CF_ACCESS_SECRET_BYTES,
                defaults::DEFAULT_CF_ACCESS_SECRET_KEY,
            )
        } else {
            secret
        },
    )
}
