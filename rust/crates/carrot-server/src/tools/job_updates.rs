use super::{jobs::Store, text};
use crate::{json_fields::set, Error, Value};
use num_traits::ToPrimitive;

pub struct Progress<'a> {
    pub message: Option<&'a str>,
    pub current: Option<i64>,
    pub total: Option<i64>,
}
impl Store {
    pub fn append(&self, id: &str, text: &Value) -> Result<(), Error> {
        if matches!(text, Value::Null) {
            return Ok(());
        }
        let Value::Text(chunk) = text.py_string()? else {
            return Ok(());
        };
        let chunk = text::lines(&chunk);
        if chunk.is_empty() {
            return Ok(());
        }
        self.change(id, |job| {
            let mut points = match job.get("log") {
                Value::Text(points) => points.clone(),
                _ => Vec::new(),
            };
            if !points.is_empty() && points.last() != Some(&10) && chunk.first() != Some(&10) {
                points.push(10);
            }
            points.extend(chunk);
            set(job, "log", text::trimmed_log(&Value::Text(points)))
        })
    }
    pub fn progress(&self, id: &str, progress: Progress<'_>) -> Result<(), Error> {
        self.change(id, |job| {
            if let Some(message) = progress.message {
                set(job, "message", Value::text(message))?;
            }
            if let Some(current) = progress.current {
                set(job, "step_current", Value::integer(current.max(0)))?;
            }
            if let Some(total) = progress.total {
                set(job, "step_total", Value::integer(total.max(0)))?;
            }
            let percent = match (job.get("step_current"), job.get("step_total")) {
                (Value::Integer(current), Value::Integer(total)) if *total > 0.into() => {
                    let number = current.to_f64().unwrap_or(0.)
                        / total.to_f64().unwrap_or(f64::INFINITY)
                        * 100.;
                    Value::Float(number.clamp(0., 100.).round_ties_even())
                        .int()
                        .map(Value::Integer)?
                }
                _ => Value::Null,
            };
            set(job, "progress", percent)
        })
    }
    pub fn finish(&self, id: &str, ok: bool, result: Value) -> Result<(), Error> {
        self.change(id, |job| {
            let result = if result.truth() {
                result
            } else {
                Value::object([("ok", Value::Bool(ok))])
            };
            let error = if result.get("error").truth() {
                result.get("error").clone()
            } else if !result.get("ok").truth() && (!result.has("ok") && !ok || result.has("ok")) {
                result.get("out").clone()
            } else {
                Value::Null
            };
            for (name, value) in [
                ("status", Value::text(if ok { "done" } else { "failed" })),
                ("error", error),
                ("error_code", result.get("error_code").clone()),
                ("error_detail", result.get("error_detail").clone()),
                ("result", result),
            ] {
                set(job, name, value)?;
            }
            if ok {
                set(job, "progress", Value::integer(100))?;
            }
            Ok(())
        })?;
        self.prune()?;
        self.persist();
        Ok(())
    }
    pub fn result(&self, id: &str, rc: i32) -> Result<Value, Error> {
        let job = self
            .get(id)?
            .ok_or_else(|| Error::Source("Tools job missing".into()))?;
        let raw = crate::param_changes::text::stripped(job.get("log"), true)?;
        Ok(Value::object([
            ("ok", Value::Bool(rc == 0)),
            ("rc", Value::integer(rc)),
            (
                "out",
                if raw.truth() {
                    raw.clone()
                } else {
                    Value::text("(no output)")
                },
            ),
            ("empty_output", Value::Bool(!raw.truth())),
        ]))
    }
    pub fn create(
        &self,
        action: Value,
        payload: Value,
        notice: Option<Value>,
    ) -> Result<String, Error> {
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
        let now = Value::Float((self.clock)());
        let is_notice = notice.is_some();
        let text = notice.unwrap_or_else(|| Value::text(""));
        self.insert(Value::object([
            ("id", Value::text(&id)),
            ("action", action),
            ("payload", payload),
            (
                "status",
                Value::text(if is_notice { "done" } else { "running" }),
            ),
            ("log", text.clone()),
            ("progress", Value::integer(if is_notice { 100 } else { 0 })),
            ("message", Value::text("")),
            (
                "step_current",
                Value::integer(if is_notice { 1 } else { 0 }),
            ),
            ("step_total", Value::integer(if is_notice { 1 } else { 0 })),
            ("error", Value::Null),
            ("error_code", Value::Null),
            ("error_detail", Value::Null),
            (
                "result",
                if is_notice {
                    Value::object([("ok", Value::Bool(true)), ("out", text)])
                } else {
                    Value::Null
                },
            ),
            ("created_at", now.clone()),
            ("updated_at", now),
        ]))?;
        Ok(id)
    }
}
