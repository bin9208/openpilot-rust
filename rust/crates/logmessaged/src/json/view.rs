//! Immutable views retain the parser's arbitrary integers and Python Unicode strings.
use super::{Document, Error, Text, Value};
use std::sync::Arc;

/// Owned immutable node view. Child views keep the backing parsed document alive.
#[derive(Clone)]
pub struct JsonValue {
    document: Arc<Document>,
    index: usize,
}

/// Borrowed scalar data or allocated child views, with no mutable parser indices exposed.
pub enum JsonView<'a> {
    Null,
    Bool(bool),
    Integer(&'a str),
    Float(f64),
    Text(&'a [u32]),
    Array(Vec<JsonValue>),
    Object(Vec<(&'a [u32], JsonValue)>),
}
impl JsonValue {
    /// Parse using the existing Python-compatible decoder.
    ///
    /// # Errors
    /// Returns syntax or Python integer-length errors.
    pub fn parse(source: &str) -> Result<Self, Error> {
        let document = super::parse(source)?;
        let index = document.root;
        Ok(Self {
            document: Arc::new(document),
            index,
        })
    }
    /// Construct a string from valid Rust UTF-8.
    pub fn text(source: &str) -> Self {
        Self::single(Value::Text(Text::from(source)))
    }
    /// Construct Python text, accepting Unicode scalar values and lone surrogates.
    pub fn codepoints(points: Vec<u32>) -> Option<Self> {
        points
            .iter()
            .all(|&point| point <= 0x10ffff)
            .then(|| Self::single(Value::Text(Text(points))))
    }
    fn single(value: Value) -> Self {
        Self {
            document: Arc::new(Document {
                values: vec![value],
                root: 0,
            }),
            index: 0,
        }
    }
    fn node(&self, index: usize) -> Self {
        Self {
            document: Arc::clone(&self.document),
            index,
        }
    }
    /// Inspect the node; arrays and objects allocate vectors of cheap child views.
    pub fn view(&self) -> JsonView<'_> {
        match &self.document.values[self.index] {
            Value::Null => JsonView::Null,
            Value::Bool(value) => JsonView::Bool(*value),
            Value::Integer(value) => JsonView::Integer(value),
            Value::Float(value) => JsonView::Float(*value),
            Value::Text(value) => JsonView::Text(&value.0),
            Value::Array(values) => {
                JsonView::Array(values.iter().map(|&index| self.node(index)).collect())
            }
            Value::Object(values) => JsonView::Object(
                values
                    .iter()
                    .map(|(key, index)| (key.0.as_slice(), self.node(*index)))
                    .collect(),
            ),
        }
    }
    /// Whether object lookup is valid for this value.
    pub fn is_object(&self) -> bool {
        matches!(&self.document.values[self.index], Value::Object(_))
    }
    /// Look up an object field. Missing keys and nonobjects return None.
    pub fn get(&self, name: &str) -> Option<Self> {
        match &self.document.values[self.index] {
            Value::Object(values) => values
                .iter()
                .find(|(key, _)| key.0.iter().copied().eq(name.chars().map(u32::from)))
                .map(|(_, index)| self.node(*index)),
            _ => None,
        }
    }
    /// Compare string codepoints without replacement decoding.
    pub fn text_eq(&self, other: &str) -> bool {
        match self.view() {
            JsonView::Text(points) => points.iter().copied().eq(other.chars().map(u32::from)),
            _ => false,
        }
    }
    /// None means either a nonstring value or Python text containing lone surrogates.
    pub fn to_utf8(&self) -> Option<String> {
        match self.view() {
            JsonView::Text(points) => points.iter().copied().map(char::from_u32).collect(),
            _ => None,
        }
    }
    /// Encode this node with the existing Python-compatible writer.
    ///
    /// # Errors
    /// Returns a formatting error if the underlying writer fails.
    pub fn to_json(&self) -> Result<String, std::fmt::Error> {
        let mut output = String::new();
        self.document.write_node(self.index, &mut output)?;
        Ok(output)
    }
}
