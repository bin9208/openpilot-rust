//! Separate bounded observation publications; display is never a control input.
use serde_json::{Value, json};
use std::{fs, path::Path};
pub const START_PATH: &str = "/dev/shm/carrot_signal_start_observation.json";
pub const PREPARE_PATH: &str = "/dev/shm/carrot_signal_prepare_observation.json";
pub const DISPLAY_PATH: &str = "/dev/shm/carrot_signal_display.json";
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("signal record missing or invalid {0}")]
    Field(&'static str),
    #[error("signal observation too large")]
    Size,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
fn field<'a>(value: &'a Value, key: &'static str) -> Result<&'a Value, Error> {
    value.get(key).ok_or(Error::Field(key))
}
fn tracks(record: &Value) -> Result<impl Iterator<Item = &Value>, Error> {
    Ok(field(record, "tracks")?
        .as_array()
        .ok_or(Error::Field("tracks"))?
        .iter()
        .take(20))
}
fn header(record: &Value, tracks: Vec<Value>) -> Result<Value, Error> {
    let timestamp = field(record, "timestamp_eof")?
        .as_u64()
        .ok_or(Error::Field("timestamp_eof"))? as f64
        / 1e9;
    Ok(
        json!({"version":1,"stream":"road","size":[1344,760],"session":field(record,"session")?,"frame_id":field(record,"frame_id")?,"timestamp":timestamp,"tracks":tracks}),
    )
}
pub fn start(record: &Value) -> Result<Value, Error> {
    let neural = record
        .get("neural")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut out = Vec::new();
    for t in tracks(record)? {
        let mut raw = t
            .get("evidence")
            .and_then(|e| e.get("raw"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if neural {
            let labels = t
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("")
                .split(',')
                .collect::<Vec<_>>();
            raw = if labels.contains(&"red") {
                "red"
            } else if labels == ["green"] {
                "green"
            } else {
                "unknown"
            };
            let score = t.get("score").and_then(Value::as_f64).unwrap_or(0.);
            let age = t
                .get("neural_age")
                .and_then(Value::as_f64)
                .unwrap_or(f64::INFINITY);
            if !score.is_finite() || !(0.8..=1.).contains(&score) || !(0. ..=0.5).contains(&age) {
                raw = "unknown";
            }
        } else if t.get("observations").and_then(Value::as_f64).unwrap_or(0.) < 3. {
            raw = "unknown";
        }
        if t.get("age").and_then(Value::as_f64) != Some(0.) {
            raw = "unknown";
        }
        out.push(json!({"box":field(t,"box")?,"raw":raw,"id":t.get("id"),"seen_red":t.get("seen_red").and_then(Value::as_bool).unwrap_or(false)&&!neural}));
    }
    header(record, out)
}
pub fn prepare(record: &Value) -> Result<Value, Error> {
    header(record, tracks(record)?.cloned().collect())
}
pub fn display(record: &Value) -> Result<Value, Error> {
    let mut out = Vec::new();
    for t in tracks(record)? {
        out.push(json!({"id":field(t,"id")?,"box":field(t,"box")?,"state":field(t,"state")?,"age":field(t,"age")?,"raw":field(field(t,"evidence")?,"raw")?}));
    }
    let mut value = header(record, out)?;
    let engine = if record
        .get("neural")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        format!(
            "neural_{}",
            field(record, "backend")?
                .as_str()
                .ok_or(Error::Field("backend"))?
        )
    } else if record
        .get("revalidated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || record
            .get("daytime_comparison")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        "trial".into()
    } else {
        "legacy".into()
    };
    value["engine"] = engine.into();
    Ok(value)
}
pub fn write(path: &Path, value: &Value, max_bytes: usize) -> Result<(), Error> {
    let text = serde_json::to_string(value)?;
    if text.len() > max_bytes {
        return Err(Error::Size);
    }
    let temp = path.with_extension("tmp");
    fs::write(&temp, text)?;
    fs::rename(temp, path)?;
    Ok(())
}
/// Neural visualization evidence; this does not assign ego-lane permission.
pub fn neural_tracks(tracks: &[Value]) -> Result<Vec<Value>, Error> {
    let mut output = Vec::new();
    for track in tracks.iter().take(20) {
        let labels = field(track, "label")?
            .as_str()
            .ok_or(Error::Field("label"))?
            .split(',')
            .collect::<Vec<_>>();
        let score = field(track, "score")?
            .as_f64()
            .ok_or(Error::Field("score"))?;
        let raw = if score >= 0.5 {
            if labels.contains(&"red") {
                "red"
            } else if labels == ["green"] {
                "green"
            } else {
                "unknown"
            }
        } else {
            "unknown"
        };
        output.push(json!({"id":field(track,"id")?,"box":field(track,"box")?,"age":0,"state":"unknown","evidence":{"raw":raw},"label":field(track,"label")?,"score":field(track,"score")?,"source":field(track,"source")?,"neural_age":field(track,"neural_age")?}));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publications_bound_tracks_and_neural_cannot_inherit_red_history() {
        let record = json!({"timestamp_eof":10_000_000_000_u64,"frame_id":30,"session":"s","neural":true,"backend":"qcom","tracks":(0..24).map(|id|json!({"id":id,"box":[600,200,640,210],"state":"green","age":0,"observations":20,"seen_red":true,"label":"green","score":0.9,"neural_age":0.1,"evidence":{"raw":"green"}})).collect::<Vec<_>>()});
        let payload = start(&record).unwrap();
        assert_eq!(payload["tracks"].as_array().unwrap().len(), 20);
        assert_eq!(payload["tracks"][0]["seen_red"], false);
        assert_eq!(display(&record).unwrap()["engine"], "neural_qcom");
        assert_eq!(
            prepare(&record).unwrap()["tracks"]
                .as_array()
                .unwrap()
                .len(),
            20
        );
    }
}
