//! Private provisioning. Payloads must never enter Params, status, telemetry or logs.
use crate::Error;
use serde_json::{json, Value};
use std::collections::HashSet;
mod publisher;
mod worker;
pub use publisher::Publisher;
pub use worker::run;
pub const MESSAGE: u16 = 0x4002;
pub const LIMIT: usize = 16384;

pub fn validate(value: &Value) -> Result<(), Error> {
    let invalid = || Error::Contract("invalid private provisioning payload");
    let object = value.as_object().ok_or_else(invalid)?;
    if object.get("version").and_then(Value::as_f64) != Some(1.0) && value["version"] != true {
        return Err(invalid());
    }
    let owner = value["owner"].as_str().ok_or_else(invalid)?;
    if owner.len() != 64
        || !owner
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err(invalid());
    }
    let profiles = value["profiles"]
        .as_array()
        .filter(|v| v.len() <= 8)
        .ok_or_else(invalid)?;
    let mut seen = HashSet::new();
    for profile in profiles {
        let p = profile.as_object().ok_or_else(invalid)?;
        if p.len() != 6
            || !["id", "ssid", "security", "password", "hidden", "active"]
                .iter()
                .all(|key| p.contains_key(*key))
        {
            return Err(invalid());
        }
        let id = profile["id"].as_str().ok_or_else(invalid)?;
        if id.len() != 36
            || !id
                .bytes()
                .all(|v| v == b'-' || v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
            || !seen.insert(id)
        {
            return Err(invalid());
        }
        let ssid = profile["ssid"]
            .as_str()
            .filter(|v| (1..=32).contains(&v.len()))
            .ok_or_else(invalid)?;
        let password = profile["password"].as_str().ok_or_else(invalid)?;
        if ssid
            .bytes()
            .chain(password.bytes())
            .any(|v| b"\0\r\n".contains(&v))
            || !profile["hidden"].is_boolean()
            || !profile["active"].is_boolean()
        {
            return Err(invalid());
        }
        let valid = match profile["security"].as_str() {
            Some("open") => password.is_empty(),
            Some("wpa-psk") => {
                (8..=63).contains(&password.len())
                    || (password.len() == 64 && password.bytes().all(|v| v.is_ascii_hexdigit()))
            }
            Some("sae") => (1..=63).contains(&password.len()),
            _ => false,
        };
        if !valid {
            return Err(invalid());
        }
    }
    if !value["onroad"].is_null() && !value["onroad"].is_boolean() {
        return Err(invalid());
    }
    Ok(())
}

/// Query failures retain `None` at the caller; they must not become an empty deletion set.
pub fn collect_with(
    mut query: impl FnMut(&[&str]) -> Result<String, Error>,
) -> Result<Vec<Value>, Error> {
    let active = query(&["-t", "-f", "UUID", "connection", "show", "--active"])?;
    let active: HashSet<&str> = active.lines().collect();
    let rows = query(&["-t", "-f", "UUID,TYPE", "connection", "show"])?;
    let mut ids: Vec<&str> = rows
        .lines()
        .filter(|row| row.ends_with(":802-11-wireless"))
        .filter_map(|row| row.split(':').next())
        .collect();
    ids.sort_by_key(|uid| !active.contains(uid));
    let mut profiles = Vec::new();
    for uid in ids.into_iter().take(8) {
        if uid.len() != 36
            || !uid
                .bytes()
                .all(|v| v == b'-' || v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
        {
            continue;
        }
        let output = query(&["--show-secrets","-g","802-11-wireless.ssid,802-11-wireless.mode,802-11-wireless.hidden,802-11-wireless-security.key-mgmt,802-11-wireless-security.psk","connection","show","uuid",uid])?;
        let mut fields: Vec<&str> = output.split('\n').collect();
        while fields.len() < 5 {
            fields.push("");
        }
        if fields.len() != 5 || !["infrastructure", ""].contains(&fields[1]) {
            continue;
        }
        if ["wpa-psk", "sae"].contains(&fields[3]) && fields[4].is_empty() {
            return Err(Error::Contract("saved Wi-Fi secret unavailable"));
        }
        let p = json!({"id":uid,"ssid":fields[0],"hidden":(["yes","true"].contains(&fields[2])),"active":active.contains(uid),"security":if fields[3].is_empty(){"open"}else{fields[3]},"password":fields[4]});
        if validate(&json!({"version":1,"owner":"0".repeat(64),"profiles":[p.clone()]})).is_ok() {
            profiles.push(p);
        }
    }
    Ok(profiles)
}
