use std::path::PathBuf;

pub(super) trait ArgumentValue: Sized {
    fn parse_argument(value: &str) -> Result<Self, String>;
}
impl ArgumentValue for String {
    fn parse_argument(value: &str) -> Result<Self, String> {
        Ok(value.into())
    }
}
impl ArgumentValue for PathBuf {
    fn parse_argument(value: &str) -> Result<Self, String> {
        let prefix = if value.starts_with("//") && !value.starts_with("///") {
            "//"
        } else if value.starts_with('/') {
            "/"
        } else {
            ""
        };
        let parts = value
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect::<Vec<_>>()
            .join("/");
        let normalized = format!("{prefix}{parts}");
        Ok(if normalized.is_empty() {
            ".".into()
        } else {
            normalized.into()
        })
    }
}
impl ArgumentValue for f64 {
    fn parse_argument(value: &str) -> Result<Self, String> {
        openpilot_runtime_core::python_float::parse(value)
            .ok_or_else(|| format!("invalid float value: {value:?}"))
    }
}
impl ArgumentValue for i64 {
    fn parse_argument(value: &str) -> Result<Self, String> {
        openpilot_ui_application::params::typed::integer_text(value)
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| format!("invalid or unrepresentable int value: {value:?}"))
    }
}
impl<T: ArgumentValue> ArgumentValue for Option<T> {
    fn parse_argument(value: &str) -> Result<Self, String> {
        T::parse_argument(value).map(Some)
    }
}
