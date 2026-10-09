use super::{defaults_for_capability, WebSettings};
use crate::{json_fields::set, Error, Value};

pub fn is_known_capability(id: &str) -> bool {
    id == "web_lab"
}
pub fn capability_client_spec() -> Value {
    Value::Array(vec![Value::object([
        ("id", Value::text("web_lab")),
        ("settingKey", Value::text("web_lab_enabled")),
        ("labelKey", Value::text("web_lab")),
        ("lockedLabelKey", Value::text("web_lab_locked")),
    ])])
}
pub fn resolve_capabilities(settings: &Value) -> Value {
    Value::object([(
        "web_lab",
        Value::Bool(settings.get("web_lab_enabled").truth()),
    )])
}

impl WebSettings {
    pub fn set_capability(&self, id: &str, enabled: bool) -> Result<Value, Error> {
        if !is_known_capability(id) {
            return Err(Error::Source(format!("unknown web capability: {id}")));
        }
        let current = self.read()?;
        let mut updates = Value::object([("web_lab_enabled", Value::Bool(enabled))]);
        if !enabled || !current.get("web_lab_enabled").truth() {
            for field in ["vision_ar_enabled", "vision_ar_debug"] {
                set(
                    &mut updates,
                    field,
                    defaults_for_capability(id).get(field).clone(),
                )?;
            }
        }
        let settings = self.update(&updates)?;
        Ok(Value::object([
            ("id", Value::text(id)),
            (
                "enabled",
                Value::Bool(settings.get("web_lab_enabled").truth()),
            ),
            ("settings", settings),
        ]))
    }
}
