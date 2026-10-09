use crate::{
    param_changes::text,
    param_coercion::{python_float, rounded},
    params::infer_type,
    Error, Value,
};

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Unknown,
    Bool,
    Int,
    Float,
    String,
    Time,
    Json,
    Bytes,
}
impl Kind {
    pub fn inferred(definition: &Value) -> Self {
        if !definition.truth() {
            return Self::Unknown;
        }
        match infer_type(definition) {
            "bool" => Self::Bool,
            "int" => Self::Int,
            "float" => Self::Float,
            _ => Self::String,
        }
    }
    pub fn resolve(name: &str, definition: &Value) -> Self {
        match openpilot_params::metadata(name).map(|info| info.kind) {
            Some(1) => Self::Bool,
            Some(2) => Self::Int,
            Some(3) => Self::Float,
            Some(4) => Self::Time,
            Some(5) => Self::Json,
            Some(6) => Self::Bytes,
            Some(_) => Self::String,
            None => Self::inferred(definition),
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Bool => "BOOL",
            Self::Int => "INT",
            Self::Float => "FLOAT",
            Self::String => "STRING",
            Self::Time => "TIME",
            Self::Json => "JSON",
            Self::Bytes => "BYTES",
        }
    }
    pub fn normalize(self, value: &Value) -> Result<Value, Error> {
        match self {
            Self::Bool => {
                if let Value::Text(points) = text::stripped(value, false)? {
                    let normalized = Value::Text(
                        points
                            .into_iter()
                            .map(|point| {
                                if (65..=90).contains(&point) {
                                    point + 32
                                } else {
                                    point
                                }
                            })
                            .collect(),
                    );
                    if ["1", "true", "on", "yes"]
                        .iter()
                        .any(|choice| normalized.text_eq(choice))
                    {
                        return Ok(Value::Bool(true));
                    }
                    if ["0", "false", "off", "no", ""]
                        .iter()
                        .any(|choice| normalized.text_eq(choice))
                        && matches!(value, Value::Text(_))
                    {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(value.truth()))
            }
            Self::Int => Ok(Value::Integer(rounded(value)?)),
            Self::Float => Ok(Value::Float(python_float(value)?)),
            Self::Unknown | Self::String | Self::Time | Self::Json | Self::Bytes => {
                Ok(value.py_string()?)
            }
        }
    }
    pub fn equal(self, left: &Value, right: &Value) -> Result<bool, Error> {
        match (self.normalize(left), self.normalize(right)) {
            (Ok(left), Ok(right)) => match self {
                Self::Float => Ok((left.float()? - right.float()?).abs() < 0.000001),
                Self::Unknown
                | Self::Bool
                | Self::Int
                | Self::String
                | Self::Time
                | Self::Json
                | Self::Bytes => Ok(text::equal(&left, &right)),
            },
            (Ok(_) | Err(_), Err(_)) | (Err(_), Ok(_)) => {
                Ok(text::equal(&left.py_string()?, &right.py_string()?))
            }
        }
    }
}
