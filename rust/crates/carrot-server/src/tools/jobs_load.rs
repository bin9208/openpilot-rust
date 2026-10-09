use super::{jobs::Store, text};
use crate::{json_fields::set, Error, Value};

pub(super) fn load(store: &Store) -> Result<(), Error> {
    let content = match std::fs::read_to_string(&store.path) {
        Ok(content) => content,
        Err(_) => return Ok(()),
    };
    let content = match Value::parse(&content) {
        Ok(content) => content,
        Err(_) => return Ok(()),
    };
    let raw = if matches!(content, Value::Object(_)) {
        content.get("jobs")
    } else {
        &content
    };
    let Value::Array(values) = raw else {
        return Ok(());
    };
    let mut jobs = store.lock()?;
    for raw in values {
        if let Some(job) = sanitize(raw, (store.clock)())? {
            if let Some(existing) = jobs.iter_mut().find(|old| old.get("id") == job.get("id")) {
                *existing = job;
            } else {
                jobs.push(job);
            }
        }
    }
    drop(jobs);
    if store.prune()? {
        store.persist();
    }
    Ok(())
}
fn sanitize(raw: &Value, now: f64) -> Result<Option<Value>, Error> {
    if !matches!(raw, Value::Object(_)) {
        return Ok(None);
    }
    let id = crate::param_changes::text::stripped(raw.get("id"), true)?;
    let action = crate::param_changes::text::stripped(raw.get("action"), true)?;
    if !id.truth() || !action.truth() {
        return Ok(None);
    }
    let status = crate::param_changes::text::stripped(raw.get("status"), true)?;
    let mut job = Value::object([
        ("id", id),
        ("action", action),
        (
            "payload",
            if matches!(raw.get("payload"), Value::Object(_)) {
                raw.get("payload").clone()
            } else {
                Value::object([])
            },
        ),
        (
            "status",
            Value::text(if status.text_eq("running") || status.text_eq("failed") {
                "failed"
            } else {
                "done"
            }),
        ),
        (
            "log",
            text::trimmed_log(&crate::param_changes::text::string(raw.get("log"), true)?),
        ),
        (
            "message",
            crate::param_changes::text::string(raw.get("message"), true)?,
        ),
        (
            "result",
            if matches!(raw.get("result"), Value::Object(_)) {
                raw.get("result").clone()
            } else {
                Value::Null
            },
        ),
    ]);
    for key in [
        "progress",
        "step_current",
        "step_total",
        "error",
        "error_code",
        "error_detail",
    ] {
        set(&mut job, key, raw.get(key).clone())?;
    }
    for (key, other) in [("created_at", "updated_at"), ("updated_at", "created_at")] {
        let value = if raw.get(key).truth() {
            raw.get(key)
        } else if raw.get(other).truth() {
            raw.get(other)
        } else {
            &Value::Null
        };
        set(
            &mut job,
            key,
            Value::Float(if matches!(value, Value::Null) {
                now
            } else {
                text::float(value)
            }),
        )?;
    }
    if raw.get("status").text_eq("running") {
        if !job.get("error").truth() {
            set(
                &mut job,
                "error",
                Value::text("server restarted before job completed"),
            )?;
        }
        if !job.get("message").truth() {
            set(
                &mut job,
                "message",
                Value::text("Interrupted by server restart"),
            )?;
        }
        if !job.get("result").truth() {
            let error = job.get("error").clone();
            set(
                &mut job,
                "result",
                Value::object([("ok", Value::Bool(false)), ("error", error)]),
            )?;
        }
    }
    Ok(Some(job))
}
