use crate::{Error, Value};
use openpilot_web_upload::{normalize_base_url, DEFAULT_WEB_UPLOAD_URL};

pub(crate) fn whitespace(point: &u32) -> bool {
    char::from_u32(*point).is_some_and(char::is_whitespace) || (0x1c..=0x1f).contains(point)
}

pub(crate) fn stripped(value: &Value, empty_when_false: bool) -> Result<Vec<u32>, Error> {
    if matches!(value, Value::Null) || (empty_when_false && !value.truth()) {
        return Ok(Vec::new());
    }
    let Value::Text(points) = value.py_string()? else {
        return Err(Error::Source("expected Python string".into()));
    };
    let start = points
        .iter()
        .position(|point| !whitespace(point))
        .unwrap_or(points.len());
    let end = points
        .iter()
        .rposition(|point| !whitespace(point))
        .map_or(start, |position| position + 1);
    Ok(points[start..end].to_vec())
}

pub(crate) fn lowered(value: &Value, empty_when_false: bool) -> Result<Value, Error> {
    let mut output = Vec::new();
    for point in stripped(value, empty_when_false)? {
        match char::from_u32(point) {
            Some(character) => output.extend(character.to_lowercase().map(u32::from)),
            None => output.push(point),
        }
    }
    Ok(Value::Text(output))
}

pub(crate) fn boolean(value: &Value) -> Result<bool, Error> {
    match value {
        Value::Text(_) => {
            let normalized = lowered(value, false)?;
            Ok(["1", "true", "yes", "on"]
                .iter()
                .any(|choice| normalized.text_eq(choice)))
        }
        Value::Null
        | Value::Bool(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Array(_)
        | Value::Object(_) => Ok(value.truth()),
    }
}

pub(crate) fn language(value: &Value) -> Result<Value, Error> {
    let normalized = lowered(value, true)?;
    for (alias, language) in [
        ("main_ko", "ko"),
        ("main_en", "en"),
        ("main_zh-chs", "zh"),
        ("main_zh-cht", "zh"),
    ] {
        if normalized.text_eq(alias) {
            return Ok(Value::text(language));
        }
    }
    let Value::Text(points) = &normalized else {
        return Err(Error::Source("expected language text".into()));
    };
    for language in ["ko", "zh", "en"] {
        if points
            .iter()
            .copied()
            .take(2)
            .eq(language.chars().map(u32::from))
        {
            return Ok(Value::text(language));
        }
    }
    Ok(Value::text(""))
}

pub(crate) fn ratio(value: &Value, fallback: f64) -> Result<Value, Error> {
    let ratio = match value.float() {
        Ok(number) => number,
        Err(error) if error.kind == "TypeError" || error.kind == "ValueError" => fallback,
        Err(error) => return Err(error.into()),
    };
    let ratio = if ratio.is_nan() { 0.3 } else { ratio };
    let ratio = (ratio.clamp(0.3, 0.7) / 0.05).round_ties_even() * 0.05;
    Ok(Value::text(&format!("{ratio:.2}")))
}

pub(crate) fn kmap_url(value: &Value) -> Result<Value, Error> {
    let points = stripped(value, true)?;
    Ok(if points.is_empty() {
        Value::text("https://jominki354.github.io/kmap/")
    } else {
        Value::Text(points)
    })
}

pub(crate) fn upload_url(value: &Value) -> Result<Value, Error> {
    let points = stripped(value, true)?;
    // A surrogate cannot occur in a supported http(s) ASCII default host. Other
    // URLs retain Python text so UTF-8 persistence fails at the write boundary.
    let Some(text) = points
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<String>>()
    else {
        let prefix = points.iter().copied().take(8).collect::<Vec<_>>();
        return Ok(
            if prefix.starts_with(&"http://".chars().map(u32::from).collect::<Vec<_>>())
                || prefix.starts_with(&"https://".chars().map(u32::from).collect::<Vec<_>>())
            {
                Value::Text(
                    points
                        .into_iter()
                        .rev()
                        .skip_while(|point| *point == 47)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect(),
                )
            } else {
                Value::text(DEFAULT_WEB_UPLOAD_URL)
            },
        );
    };
    let normalized = normalize_base_url(&text, DEFAULT_WEB_UPLOAD_URL)
        .unwrap_or_else(|_| DEFAULT_WEB_UPLOAD_URL.into());
    let legacy_comparison = normalized.to_lowercase().replace('\u{17f}', "s");
    let normalized = if ["https://op.wjcloud.kr", "https://shind0.synology.me"]
        .contains(&legacy_comparison.as_str())
    {
        DEFAULT_WEB_UPLOAD_URL
    } else {
        &normalized
    };
    Ok(Value::text(normalized))
}
