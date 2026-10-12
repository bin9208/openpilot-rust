use super::{coercion, Catalog};
use crate::{Error, Value};
use openpilot_web_upload::DEFAULT_WEB_UPLOAD_URL;

pub(crate) enum Kind {
    Bool,
    Enum(&'static [&'static str]),
    Language,
    Content,
    Ratio(f64),
    KmapUrl,
    UploadUrl,
}
pub(crate) enum DefaultValue {
    Bool(bool),
    Text(&'static str),
}
pub(crate) struct Field {
    pub key: &'static str,
    kind: Kind,
    default: DefaultValue,
    gated: bool,
}
impl Field {
    const fn boolean(key: &'static str, gated: bool) -> Self {
        Self {
            key,
            kind: Kind::Bool,
            default: DefaultValue::Bool(false),
            gated,
        }
    }
    const fn text(key: &'static str, kind: Kind, default: &'static str) -> Self {
        Self {
            key,
            kind,
            default: DefaultValue::Text(default),
            gated: false,
        }
    }
    pub fn default(&self) -> Value {
        match self.default {
            DefaultValue::Bool(value) => Value::Bool(value),
            DefaultValue::Text(value) => Value::text(value),
        }
    }
    pub fn coerce(&self, value: &Value) -> Result<Value, Error> {
        match self.kind {
            Kind::Bool => Ok(Value::Bool(coercion::boolean(value)?)),
            Kind::Enum(choices) => {
                let candidate = coercion::lowered(value, false)?;
                Ok(choices
                    .iter()
                    .find(|choice| candidate.text_eq(choice))
                    .map_or_else(|| self.default(), |choice| Value::text(choice)))
            }
            Kind::Language => coercion::language(value),
            Kind::Content => coercion::lowered(value, false),
            Kind::Ratio(fallback) => coercion::ratio(value, fallback),
            Kind::KmapUrl => coercion::kmap_url(value),
            Kind::UploadUrl => coercion::upload_url(value),
        }
    }
}

const MODES: &[&str] = &["area_1", "area_2", "split"];
pub(crate) const FIELDS: &[Field] = &[
    Field::boolean("auto_update_git_pull", false),
    Field::text(
        "auto_update_reboot",
        Kind::Enum(&["disengaged", "off", "park"]),
        "off",
    ),
    Field::text(
        "start_page",
        Kind::Enum(&["carrot", "last", "logs", "setting", "terminal", "tools"]),
        "last",
    ),
    Field::boolean("mini_hud_enabled", false),
    Field::text("web_language", Kind::Language, ""),
    Field::boolean("web_lab_enabled", false),
    Field::boolean("vision_fullscreen_default", false),
    Field::boolean("carrot_navi_fullscreen_on_tap", false),
    Field::boolean("vision_ar_enabled", true),
    Field::boolean("vision_ar_debug", true),
    Field::text(
        "vision_display_mode",
        Kind::Enum(&["crop", "fit", "normal"]),
        "normal",
    ),
    Field::boolean("replay_hud_visible", false),
    Field::text(
        "replay_insights_tab",
        Kind::Enum(&["advanced", "events", "graphs", "sensors"]),
        "events",
    ),
    Field::text("carrot_navi_horizontal_mode", Kind::Enum(MODES), "area_1"),
    Field::text("carrot_navi_horizontal_area_1", Kind::Content, "vision"),
    Field::text("carrot_navi_horizontal_area_2", Kind::Content, "navigation"),
    Field::text("carrot_navi_split_ratio", Kind::Ratio(0.7), "0.70"),
    Field::text("carrot_navi_vertical_mode", Kind::Enum(MODES), "area_1"),
    Field::text("carrot_navi_vertical_area_1", Kind::Content, "vision"),
    Field::text("carrot_navi_vertical_area_2", Kind::Content, "navigation"),
    Field::text("carrot_navi_vertical_split_ratio", Kind::Ratio(0.5), "0.50"),
    Field::boolean("kmap_enabled", false),
    Field::text(
        "kmap_url",
        Kind::KmapUrl,
        "https://jominki354.github.io/kmap/",
    ),
    Field::boolean("kmap_overlay_heading_up", false),
    Field::boolean("kmap_overlay_curvature_color", false),
    Field::text(
        "kmap_map_type",
        Kind::Enum(&["hybrid", "roadmap", "satellite"]),
        "roadmap",
    ),
    Field::text("web_upload_url", Kind::UploadUrl, DEFAULT_WEB_UPLOAD_URL),
    Field::text(
        "support_permission_mode",
        Kind::Enum(&["allow_all", "approve_each"]),
        "approve_each",
    ),
    Field::text(
        "support_ttl_seconds",
        Kind::Enum(&["1800", "3600", "900"]),
        "1800",
    ),
    Field::text(
        "support_command_timeout_seconds",
        Kind::Enum(&["120", "15", "30", "60"]),
        "30",
    ),
];

pub fn defaults() -> Value {
    Value::Object(
        FIELDS
            .iter()
            .map(|field| (field.key.chars().map(u32::from).collect(), field.default()))
            .collect(),
    )
}

pub fn defaults_for_capability(capability: &str) -> Value {
    Value::Object(
        FIELDS
            .iter()
            .filter(|field| field.gated && capability == "web_lab")
            .map(|field| (field.key.chars().map(u32::from).collect(), field.default()))
            .collect(),
    )
}

pub fn client_spec(catalog: &Catalog) -> Value {
    Value::Array(
        FIELDS
            .iter()
            .map(|field| {
                let kind = match field.kind {
                    Kind::Bool => "bool",
                    Kind::Enum(_) | Kind::Content => "enum",
                    Kind::Language | Kind::Ratio(_) | Kind::KmapUrl | Kind::UploadUrl => "str",
                };
                let mut entries = vec![
                    (
                        "key".chars().map(u32::from).collect(),
                        Value::text(field.key),
                    ),
                    ("type".chars().map(u32::from).collect(), Value::text(kind)),
                    ("default".chars().map(u32::from).collect(), field.default()),
                ];
                if field.gated {
                    entries.push((
                        "requiresCapability".chars().map(u32::from).collect(),
                        Value::text("web_lab"),
                    ));
                }
                let choices = match field.kind {
                    Kind::Content => Some(
                        catalog
                            .contents
                            .iter()
                            .map(|content| Value::text(&content.id))
                            .collect(),
                    ),
                    Kind::Enum(choices) => {
                        Some(choices.iter().map(|choice| Value::text(choice)).collect())
                    }
                    Kind::Bool
                    | Kind::Language
                    | Kind::Ratio(_)
                    | Kind::KmapUrl
                    | Kind::UploadUrl => None,
                };
                if let Some(choices) = choices {
                    entries.push((
                        "choices".chars().map(u32::from).collect(),
                        Value::Array(choices),
                    ));
                }
                Value::Object(entries)
            })
            .collect(),
    )
}
