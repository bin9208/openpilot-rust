use crate::{
    json::{self, Document, Text, Value},
    Error,
};

fn suffix(value: &Value) -> &'static str {
    match value {
        Value::Null | Value::Object(_) => "",
        Value::Bool(_) => "$b",
        Value::Integer(_) => "$i",
        Value::Float(_) => "$f",
        Value::Text(_) => "$s",
        Value::Array(_) => "$a",
    }
}
fn fix_keys(document: &mut Document, root: usize) {
    let mut stack = vec![root];
    while let Some(index) = stack.pop() {
        let values = match &mut document.values[index] {
            Value::Object(values) => std::mem::take(values),
            Value::Null
            | Value::Bool(_)
            | Value::Integer(_)
            | Value::Float(_)
            | Value::Text(_)
            | Value::Array(_) => continue,
        };
        let mut fixed = Vec::with_capacity(values.len());
        for (mut key, value) in values {
            key.append(suffix(&document.values[value]));
            json::insert(&mut fixed, key, value);
            stack.push(value);
        }
        document.values[index] = Value::Object(fixed);
    }
}

/// Format the original JSON record for disk, preserving Python key/type rules.
pub fn format_record(record: &str, id: uuid::Uuid) -> Result<String, Error> {
    let mut document = json::parse(record)?;
    let Value::Object(mut object) =
        std::mem::replace(&mut document.values[document.root], Value::Null)
    else {
        return Err(json::Error::Message.into());
    };
    let index = object
        .iter()
        .position(|(key, _)| *key == Text::from("msg"))
        .ok_or(json::Error::Message)?;
    let (mut key, value) = object.remove(index);
    key.append(suffix(&document.values[value]));
    fix_keys(&mut document, value);
    json::insert(&mut object, key, value);
    let id = document.push(Value::Text(Text::from(id.simple().to_string().as_str())));
    json::insert(&mut object, Text::from("id"), id);
    document.values[document.root] = Value::Object(object);
    let mut output = String::new();
    document.write(&mut output)?;
    Ok(output)
}
