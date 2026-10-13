use super::{integer, text, unsigned, Error, Projection};
use crate::models::{NaviDashboardState, NaviItemStatus};
use num_traits::ToPrimitive;
use openpilot_carrot_navi::{json::Value, manifest::CATALOG};

impl Projection {
    pub(super) fn dashboard(
        &self,
        metadata: &Value,
        wall_ms: i128,
    ) -> Result<NaviDashboardState, Error> {
        let received = metadata
            .get("last_received_at_ms")
            .int()?
            .to_i128()
            .ok_or(Error::Range)?;
        let age = if received > 0 {
            Some(i64::try_from((wall_ms - received).max(0)).map_err(|_| Error::Range)?)
        } else {
            None
        };
        let items = CATALOG
            .iter()
            .map(|(kind, name)| {
                let key = format!("{kind}:{name}");
                Ok(if let Some(record) = self.last_records.get(&key) {
                    NaviItemStatus {
                        key,
                        sequence: unsigned(record.get("sequence"))?,
                        present: record.get("present").truth(),
                        reason: text(record.get("reason"))?,
                    }
                } else {
                    NaviItemStatus {
                        key,
                        reason: Some("waiting".into()),
                        ..NaviItemStatus::default()
                    }
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(NaviDashboardState {
            connected: metadata.get("connected").truth(),
            endpoint: self.endpoint.clone(),
            session_id: metadata.get("session_id").string()?,
            app_version: metadata.get("app_version").string()?,
            manifest_revision: integer(metadata.get("manifest_revision"))?,
            received_count: integer(metadata.get("received_count"))?,
            last_received_age_ms: age,
            map_frame_age_ms: self.map_age,
            map_stream_stalled: self.stalled,
            peer: metadata.get("peer").string()?,
            error: if metadata.get("error").truth() {
                text(metadata.get("error"))?
            } else {
                None
            },
            media: self.media.values().map(|frame| (**frame).clone()).collect(),
            items,
            app_status: app_status(self.value("json:app_status")),
            camera_status: camera_status(self.value("json:camera_state"))?,
            composition_status: composition_status(self.value("json:composition_state"))?,
        })
    }

    fn value(&self, key: &str) -> &Value {
        self.last_records
            .get(key)
            .filter(|record| record.get("present").truth())
            .map(|record| record.get("value"))
            .filter(|value| matches!(value, Value::Object(_)))
            .unwrap_or(&Value::Null)
    }

    /// # Errors
    /// Returns receiver metadata conversion errors.
    pub fn status_text(&self) -> Result<String, Error> {
        let Some(metadata) = &self.last_metadata else {
            return Ok(format!("navi {} starting", self.endpoint));
        };
        let sequence = self
            .last_records
            .get("render:map_main")
            .map_or(Ok(0), |record| unsigned(record.get("sequence")))?;
        let connected = u8::from(metadata.get("connected").truth());
        let app = if metadata.get("app_version").truth() {
            metadata.get("app_version").string()?
        } else {
            "-".into()
        };
        let received = integer(metadata.get("received_count"))?;
        let status = if self.stalled {
            "stalled".into()
        } else {
            self.map_age
                .map_or_else(|| "-".into(), |age| format!("{age}ms"))
        };
        let drops = self.worker.dropped_requests();
        let decode = if drops > 0 {
            format!(" decode_drop={drops}")
        } else {
            String::new()
        };
        Ok(format!("navi connected={connected} app={app} items={}/28 received={received} map={sequence}/{status}{decode}", self.last_records.len()))
    }
}

fn app_status(value: &Value) -> String {
    if !value.truth() {
        return "APP waiting".into();
    }
    let words: Vec<_> = [
        ("foreground", "FG", "BG"),
        ("window_focused", "FOCUS", "NO-FOCUS"),
        ("main_map_visible", "MAP", "NO-MAP"),
        ("ui_capture_available", "CAPTURE", "NO-CAPTURE"),
    ]
    .into_iter()
    .map(|(key, yes, no)| if value.get(key).truth() { yes } else { no })
    .collect();
    format!("APP {}", words.join(" "))
}
fn camera_status(value: &Value) -> Result<String, Error> {
    if !value.truth() {
        return Ok("CAM waiting".into());
    }
    let field = |key: &str| {
        if value.has(key) {
            value.get(key).string().map_err(Error::from)
        } else {
            Ok("-".into())
        }
    };
    Ok(format!(
        "CAM {} L{}.{} T{} B{}",
        field("camera_mode")?.to_uppercase(),
        field("view_level")?,
        field("view_sub_level")?,
        field("tilt")?,
        field("bearing")?
    ))
}
fn composition_status(value: &Value) -> Result<String, Error> {
    let Value::Object(fields) = value else {
        return Ok("UI waiting".into());
    };
    if fields.is_empty() {
        return Ok("UI waiting".into());
    }
    let active = fields
        .iter()
        .filter(|(_, value)| value.truth())
        .map(|(key, _)| Value::Text(key.clone()).string())
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|key| key != "generation")
        .take(5)
        .map(|key| {
            key.strip_suffix("_active")
                .unwrap_or(&key)
                .replace('_', " ")
                .to_uppercase()
        })
        .collect::<Vec<_>>();
    Ok(format!(
        "UI {}",
        if active.is_empty() {
            "MAP ONLY".into()
        } else {
            active.join(" / ")
        }
    ))
}
