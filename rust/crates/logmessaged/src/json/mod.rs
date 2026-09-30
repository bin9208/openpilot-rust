//! Python json.loads/json.dumps-compatible values for SwagLogFileFormatter.
//! Preserve arbitrary integers, insertion order, NaN/Infinity and lone UTF-16
//! surrogates, which cannot be represented by serde_json::Value/String.
mod parse;
mod string;
mod write;
pub(crate) use parse::parse;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Text(pub Vec<u32>);
impl From<&str> for Text {
    fn from(value: &str) -> Self {
        Self(value.chars().map(u32::from).collect())
    }
}
impl Text {
    pub fn append(&mut self, suffix: &str) {
        self.0.extend(suffix.chars().map(u32::from));
    }
}
#[derive(Debug)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    Integer(String),
    Float(f64),
    Text(Text),
    Array(Vec<usize>),
    Object(Vec<(Text, usize)>),
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("JSON syntax at byte {offset}: {reason}")]
    Syntax { offset: usize, reason: &'static str },
    #[error("record must be a JSON object containing msg")]
    Message,
    #[error("integer exceeds Python's default 4300-digit input limit")]
    IntegerLimit,
}

pub(crate) fn insert(object: &mut Vec<(Text, usize)>, key: Text, value: usize) {
    if let Some((_, current)) = object.iter_mut().find(|(name, _)| *name == key) {
        *current = value;
    } else {
        object.push((key, value));
    }
}

pub(crate) struct Document {
    pub values: Vec<Value>,
    pub root: usize,
}
impl Document {
    pub fn push(&mut self, value: Value) -> usize {
        let index = self.values.len();
        self.values.push(value);
        index
    }
}

/// Read optional string fields using the original Python JSON decoder semantics.
/// Duplicate keys keep their last value; unselected nonfinite values and lone
/// surrogates remain valid. A selected non-string or non-Unicode value is absent.
pub fn string_fields<const N: usize>(
    source: &str,
    keys: [&str; N],
) -> Result<[Option<String>; N], Error> {
    let document = parse(source)?;
    let Value::Object(fields) = &document.values[document.root] else {
        return Ok(std::array::from_fn(|_| None));
    };
    Ok(std::array::from_fn(|index| {
        let key = Text::from(keys[index]);
        let (_, value) = fields.iter().find(|(name, _)| *name == key)?;
        let Value::Text(text) = &document.values[*value] else {
            return None;
        };
        text.0.iter().map(|point| char::from_u32(*point)).collect()
    }))
}
