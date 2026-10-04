//! Ordered alert catalog and Params conversion from mici/layouts/offroad_alerts.py.
use crate::{context::Context, params::Read};

#[derive(Clone, Debug, serde::Serialize)]
pub struct AlertData {
    pub key: String,
    pub text: String,
    pub severity: i32,
    pub visible: bool,
}
#[derive(serde::Deserialize)]
struct Config {
    #[serde(default)]
    severity: i32,
}
struct Catalog(Vec<(String, Config)>);
impl<'de> serde::Deserialize<'de> for Catalog {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Catalog;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("ordered offroad alert catalog")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Catalog, M::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Catalog(entries))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}
#[derive(serde::Deserialize, Default)]
struct Active {
    #[serde(default)]
    text: String,
    #[serde(default)]
    extra: String,
}
pub(super) fn catalog(context: &Context) -> Result<Vec<AlertData>, crate::Error> {
    let path = context
        .source_root
        .join("openpilot/selfdrive/selfdrived/alerts_offroad.json");
    let mut catalog: Catalog = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|error| crate::Error::Io(std::io::Error::other(error)))?;
    catalog.0.sort_by(|a, b| b.1.severity.cmp(&a.1.severity));
    let mut alerts = vec![AlertData {
        key: "UpdateAvailable".into(),
        text: String::new(),
        severity: -1,
        visible: false,
    }];
    alerts.extend(catalog.0.into_iter().map(|(key, config)| AlertData {
        key,
        text: String::new(),
        severity: config.severity,
        visible: false,
    }));
    Ok(alerts)
}
pub(super) fn refresh(context: &Context, alerts: &mut [AlertData]) -> Result<(), crate::Error> {
    for alert in alerts {
        if alert.key == "UpdateAvailable" {
            alert.visible = context.params.boolean("UpdateAvailable")?;
            alert.text = if alert.visible {
                let description = context.params.string("UpdaterNewDescription")?;
                let parts: Vec<_> = description.split(" / ").collect();
                let version = if parts.len() > 3 {
                    format!("\nopenpilot {}, {}\n", parts[0], parts[3])
                } else {
                    String::new()
                };
                format!(
                    "Update available {version}. Click to update. Read the release notes at blog.comma.ai."
                )
            } else {
                String::new()
            };
        } else {
            let bytes = context
                .params
                .bytes(&alert.key)?
                .filter(|bytes| !bytes.is_empty());
            let active = active(bytes.as_deref())?;
            alert.text = active.text.replace("%1", &active.extra);
            alert.visible = !alert.text.is_empty();
        }
    }
    Ok(())
}
fn active(bytes: Option<&[u8]>) -> Result<Active, crate::Error> {
    let Some(bytes) = bytes else {
        return Ok(Active::default());
    };
    let value: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        // Params casts malformed JSON to None before the source widget receives it.
        Err(_) => return Ok(Active::default()),
    };
    let empty = match &value {
        serde_json::Value::Null => true,
        serde_json::Value::Bool(value) => !*value,
        serde_json::Value::Number(value) => value.as_f64() == Some(0.0),
        serde_json::Value::String(value) => value.is_empty(),
        serde_json::Value::Array(value) => value.is_empty(),
        serde_json::Value::Object(value) => value.is_empty(),
    };
    if empty {
        return Ok(Active::default());
    }
    serde_json::from_value(value).map_err(|error| crate::Error::Io(std::io::Error::other(error)))
}
