use crate::Error;
use serde_json::Value;
use std::collections::BTreeMap;
pub const CONTRACT_JSON: &str =
    include_str!("../../../../openpilot/selfdrive/modeld/jetlink/cinque-v3.json");
pub const WARPED_BYTES: usize = 2 * 6 * 128 * 256;
pub const PACKED_FLOATS: usize = 12;
pub const OUTPUT_FLOATS: usize = 18452;
pub const SHA256: &str = "404a18cfd86d29637d20c697dfde245bb47c666ae016730ab674c65f4d1e1aa4";
pub type Identity = BTreeMap<String, String>;
pub fn contract() -> Result<Value, Error> {
    Ok(serde_json::from_str(CONTRACT_JSON)?)
}
pub fn validation_matches(raw: Option<&[u8]>, identity: &Identity) -> bool {
    const KEYS: [&str; 6] = [
        "android_api",
        "app_version",
        "artifact_sha256",
        "backend_requested",
        "device_model",
        "runtime_version",
    ];
    if identity.len() != KEYS.len()
        || !KEYS
            .iter()
            .all(|key| identity.get(*key).is_some_and(|v| !v.is_empty()))
    {
        return false;
    }
    let Some(raw) = raw.filter(|raw| raw.len() <= 16384) else {
        return false;
    };
    let Ok(record) = serde_json::from_slice::<Value>(raw) else {
        return false;
    };
    record["approved"] == true
        && record["device_test"] == true
        && record["numerical_parity"] == true
        && record["model_sha256"] == SHA256
        && identity.get("artifact_sha256").is_some_and(|v| v == SHA256)
        && serde_json::to_value(identity).is_ok_and(|v| record["identity"] == v)
        && record["warp_contract"] == "native-c3x-512x256-v1"
        && record["duration_seconds"]
            .as_f64()
            .is_some_and(|v| v.is_finite() && v >= 1800.0)
        && record["end_to_end_max_ms"]
            .as_f64()
            .is_some_and(|v| v.is_finite() && (0.0..=50.0).contains(&v))
        && record["deadline_misses"].as_u64() == Some(0)
        && record["validation_id"].as_str().is_some_and(|v| {
            v.len() == 64
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}
pub fn check_input(warped: &[u8], packed: &[f32]) -> Result<(), Error> {
    if warped.len() != WARPED_BYTES
        || packed.len() != PACKED_FLOATS
        || !packed.iter().all(|v| v.is_finite())
    {
        return Err(Error::Contract("input shape/finite"));
    }
    Ok(())
}
pub fn check_output(values: &[f32]) -> Result<(), Error> {
    if values.len() != OUTPUT_FLOATS || !values.iter().all(|v| v.is_finite()) {
        return Err(Error::Contract("output shape/finite"));
    }
    Ok(())
}
