//! Isolated independent registration source worker and verified observations.
use super::{values as v, Case, ReferenceOptions};
use crate::{
    qa_process::{NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{collections::BTreeMap, path::Path, time::Duration};
pub struct Observation {
    pub normalized: JsonValue,
    pub elapsed: Duration,
    pub public_keys: BTreeMap<String, String>,
}
pub fn observe(options: &ReferenceOptions, case: &Case, worker_elf: &Path) -> Result<Observation> {
    let source = super::validate_source(&options.root)?;
    let worker = super::identity::Artifact::open(worker_elf)?;
    let namespace = super::worker::Namespace::new()?;
    let mut request = v::fields(&super::request(options, case, None)?)?;
    request.insert("namespace".into(), namespace.descriptor()?);
    let captured = NativeRunner.run(&super::spec(
        worker_elf,
        "--registration-side",
        v::object(request)?,
        &options.environment,
    )?)?;
    let reply = super::worker_reply(captured)?;
    worker.verify(worker_elf)?;
    if !v::equal(&super::validate_source(&options.root)?, &source) {
        return Err("registration sources changed during source observation".into());
    }
    let elapsed = Duration::from_secs_f64(v::number(&v::get(&reply, "elapsed_seconds")?)?);
    let public_keys = v::fields(&v::get(&reply, "public_keys")?)?
        .into_iter()
        .map(|(name, value)| Ok((name, v::string(&value)?)))
        .collect::<Result<_>>()?;
    Ok(Observation {
        normalized: v::get(&reply, "normalized")?,
        elapsed,
        public_keys,
    })
}
pub fn normalize_requests(
    raw: &JsonValue,
    public_keys: &BTreeMap<String, String>,
    expiration: i64,
) -> Result<JsonValue> {
    super::http::normalized(raw, public_keys, expiration)
}
pub(super) fn normalize_result(raw: &JsonValue) -> Result<JsonValue> {
    let mut fields = v::fields(raw)?;
    let mut outcome = v::fields(&v::get(raw, "outcome")?)?;
    outcome.remove("detail");
    outcome.remove("type");
    if let Some(serialized) = outcome.remove("value_json") {
        outcome.insert("value".into(), JsonValue::parse(&v::string(&serialized)?)?);
    }
    if let Some(value) = outcome.get("value") {
        let name = match value.view() {
            JsonView::Null => "NoneType",
            JsonView::Bool(_) => "bool",
            JsonView::Integer(_) => "int",
            JsonView::Float(_) => "float",
            JsonView::Text(_) => "str",
            JsonView::Array(_) => "list",
            JsonView::Object(_) => "dict",
        };
        outcome.insert("value_type".into(), v::text(name));
    }
    fields.insert("outcome".into(), v::object(outcome)?);
    v::object(fields)
}
pub(super) fn normalize_records(raw: &JsonValue) -> Result<JsonValue> {
    v::array(
        v::items(raw)?
            .iter()
            .map(|row| {
                let msg = v::get(row, "msg")?;
                let JsonView::Text(points) = msg.view() else {
                    return Err("registration diagnostic msg is not text".into());
                };
                let end = points
                    .iter()
                    .position(|p| *p == u32::from('\n'))
                    .unwrap_or(points.len());
                v::array([
                    v::get(row, "levelnum")?,
                    JsonValue::codepoints(points[..end].to_vec())
                        .ok_or("invalid diagnostic text")?,
                    v::value(row.get("exc_info").is_some_and(|v| v::truth(&v)))?,
                ])
            })
            .collect::<Result<Vec<_>>>()?,
    )
}
