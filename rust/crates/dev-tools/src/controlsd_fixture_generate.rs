//! Native construction of controlsd_fixture.py/controlsd_scenarios.py inputs.
//! Closed 16-service fixture domain; no runtime controller or source interpreter.
use crate::seal::Result;
use openpilot_log_format::{JsonValue as J, JsonView as V};
use std::{fs, path::Path};
mod audit;
mod parameters;
mod recipes;
mod wire;
pub use audit::{audit_saved, canonical_inputs};
pub use wire::{project_event, service};

pub const SERVICES: [&str; 16] = [
    "onroadEvents",
    "carState",
    "liveParameters",
    "liveTorqueParameters",
    "liveDelay",
    "modelV2",
    "longitudinalPlan",
    "lateralPlan",
    "radarState",
    "carOutput",
    "carrotMan",
    "driverMonitoringState",
    "driverAssistance",
    "livePose",
    "liveCalibration",
    "selfdriveState",
];
pub const SOURCES: [(&str, &str); 3] = [
    (
        "rust/tools/controlsd_fixture.py",
        "df3fe9adc8b9ed195927700de5235fd88df58c0c80a617a5e043c5d5283fbb45",
    ),
    (
        "rust/tools/controlsd_scenarios.py",
        "d83dfce64a13d033143fccbeb03bd4a270dd3837f759ad58a3bdca50fe472368",
    ),
    (
        "rust/tools/paramsd_fixture.py",
        "8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521",
    ),
];
pub fn verify_sources(root: &Path) -> Result<()> {
    for (name, expected) in SOURCES {
        if !fs::symlink_metadata(root.join(name))?.is_file()
            || crate::seal::digest(&root.join(name))? != expected
        {
            return Err(format!("controlsd fixture source changed: {name}").into());
        }
    }
    Ok(())
}
pub(super) fn field(value: &J, name: &str) -> Result<J> {
    value
        .get(name)
        .ok_or_else(|| format!("missing fixture field: {name}").into())
}
pub(super) fn text(value: &J) -> Result<String> {
    value
        .to_utf8()
        .ok_or_else(|| "fixture text required".into())
}
pub(super) fn number(value: &J) -> Result<f64> {
    match value.view() {
        V::Float(v) => Ok(v),
        V::Integer(v) => Ok(v.parse()?),
        V::Bool(v) => Ok(f64::from(u8::from(v))),
        _ => Err("fixture number required".into()),
    }
}
pub(super) fn real(value: f64) -> Result<J> {
    Ok(J::parse(&if value.is_nan() {
        "NaN".into()
    } else if value == f64::INFINITY {
        "Infinity".into()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".into()
    } else {
        serde_json::to_string(&value)?
    })?)
}
pub(super) fn json<T: serde::Serialize + ?Sized>(value: &T) -> Result<J> {
    Ok(J::parse(&serde_json::to_string(value)?)?)
}
pub(super) fn array(rows: Vec<J>) -> Result<J> {
    Ok(J::parse(&format!(
        "[{}]",
        rows.iter()
            .map(J::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))?)
}
pub(super) fn entries(value: &J) -> Result<Vec<(String, J)>> {
    let V::Object(rows) = value.view() else {
        return Err("fixture object required".into());
    };
    rows.into_iter()
        .map(|(key, value)| {
            Ok((
                key.iter()
                    .copied()
                    .map(|c| char::from_u32(c).ok_or("invalid fixture key"))
                    .collect::<std::result::Result<_, _>>()?,
                value,
            ))
        })
        .collect()
}
pub(super) fn object(rows: Vec<(String, J)>) -> Result<J> {
    Ok(J::parse(&format!(
        "{{{}}}",
        rows.iter()
            .map(|(k, v)| Ok(format!("{}:{}", serde_json::to_string(k)?, v.to_json()?)))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))?)
}
pub(super) fn merge(old: &J, updates: &J) -> Result<J> {
    let mut rows = entries(old)?;
    for (name, value) in entries(updates)? {
        if let Some((_, old)) = rows.iter_mut().find(|(key, _)| *key == name) {
            *old = value;
        } else {
            rows.push((name, value));
        }
    }
    object(rows)
}
pub(super) fn bytes(value: &J) -> Result<Vec<u8>> {
    let V::Array(rows) = value.view() else {
        return Err("fixture bytes require an array".into());
    };
    rows.iter()
        .map(|v| match v.view() {
            V::Integer(v) => Ok(v.parse::<u8>()?),
            _ => Err("fixture byte must be an integer".into()),
        })
        .collect()
}
pub(super) fn python_string(value: &J) -> Result<String> {
    Ok(match value.view() {
        V::Text(_) => text(value)?,
        V::Bool(v) => if v { "True" } else { "False" }.into(),
        V::Null => "None".into(),
        V::Integer(v) => v.into(),
        V::Float(v) if v.is_nan() => "nan".into(),
        V::Float(v) if v == f64::INFINITY => "inf".into(),
        V::Float(v) if v == f64::NEG_INFINITY => "-inf".into(),
        V::Float(_) => value.to_json()?,
        _ => return Err("setting outside actual scalar fixture domain".into()),
    })
}
pub(super) fn truthy(value: &J) -> bool {
    match value.view() {
        V::Null => false,
        V::Bool(v) => v,
        V::Integer(v) => v.parse::<i128>().map_or(true, |v| v != 0),
        V::Float(v) => v != 0.,
        V::Text(v) => !v.is_empty(),
        V::Array(v) => !v.is_empty(),
        V::Object(v) => !v.is_empty(),
    }
}
pub fn generate(input: &J) -> Result<J> {
    let empty = J::parse("{}")?;
    let optional = |name| {
        input
            .get(name)
            .filter(truthy)
            .unwrap_or_else(|| empty.clone())
    };
    match text(&field(input, "op")?)?.as_str() {
        "frame" => recipes::frame(number(&field(input, "index")?)?),
        "event" => {
            let name = text(&field(input, "service")?)?;
            if name == "onroadEvents" {
                return Err("onroadEvents uses frame's explicit empty-list constructor".into());
            }
            json(&wire::event_packet(
                &name,
                number(&field(input, "time")?)?,
                &field(input, "data")?,
            )?)
        }
        "change" => {
            let omit = match input.get("omit") {
                None => Vec::new(),
                Some(v) => {
                    let V::Array(values) = v.view() else {
                        return Err("omit must be a list".into());
                    };
                    values.iter().map(text).collect::<Result<_>>()?
                }
            };
            wire::change(
                &field(input, "row")?,
                &optional("updates"),
                &omit,
                input.get("settings").filter(truthy).as_ref(),
            )
        }
        "parameters" => json(&parameters::parameters(
            &text(&field(input, "fingerprint")?)?,
            &text(&field(input, "mode")?)?,
            &optional("settings"),
        )?),
        "case" => parameters::case(
            &text(&field(input, "name")?)?,
            &text(&field(input, "fingerprint")?)?,
            &text(&field(input, "mode")?)?,
            field(input, "frames")?,
            &optional("settings"),
            &optional("cp_changes"),
        ),
        "fixture-cases" => recipes::fixture_cases(),
        "scenario-cases" => recipes::scenario_cases(),
        "cases" => {
            let V::Array(mut a) = recipes::fixture_cases()?.view() else {
                unreachable!()
            };
            let V::Array(b) = recipes::scenario_cases()?.view() else {
                unreachable!()
            };
            a.extend(b);
            array(a)
        }
        _ => Err("unknown controlsd fixture operation".into()),
    }
}
