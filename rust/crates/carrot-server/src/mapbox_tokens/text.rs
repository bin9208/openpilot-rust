use crate::{Error, Value};

pub(super) fn utf8(token: &Value) -> Result<String, Error> {
    let Value::Text(points) = token.py_string()? else {
        return Err(Error::Source("expected token text".into()));
    };
    let mut text = String::new();
    for (index, point) in points.iter().copied().enumerate() {
        if let Some(character) = char::from_u32(point) {
            text.push(character);
            continue;
        }
        let count = points[index..]
            .iter()
            .take_while(|point| (0xd800..=0xdfff).contains(*point))
            .count();
        let subject = if count == 1 {
            format!("character '\\u{point:04x}' in position {index}")
        } else {
            format!("characters in position {index}-{}", index + count - 1)
        };
        return Err(Error::Source(format!(
            "'utf-8' codec can't encode {subject}: surrogates not allowed"
        )));
    }
    Ok(text)
}
