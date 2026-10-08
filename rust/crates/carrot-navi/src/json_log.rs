use crate::{json::Value, Error};

impl Value {
    pub(crate) fn compact_sorted_points(&self) -> Result<Vec<u32>, Error> {
        let mut points = Vec::new();
        self.write_points(&mut points)?;
        Ok(points)
    }
    fn write_points(&self, points: &mut Vec<u32>) -> Result<(), Error> {
        match self {
            Self::Text(text) => write_text(text, points),
            Self::Array(values) => {
                points.push(u32::from('['));
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        points.push(u32::from(','));
                    }
                    value.write_points(points)?;
                }
                points.push(u32::from(']'));
            }
            Self::Object(fields) => {
                let mut sorted: Vec<_> = fields.iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(&b.0));
                points.push(u32::from('{'));
                for (index, (key, value)) in sorted.into_iter().enumerate() {
                    if index > 0 {
                        points.push(u32::from(','));
                    }
                    write_text(key, points);
                    points.push(u32::from(':'));
                    value.write_points(points)?;
                }
                points.push(u32::from('}'));
            }
            Self::Null | Self::Bool(_) | Self::Integer(_) | Self::Float(_) => {
                points.extend(self.encode()?.chars().map(u32::from))
            }
        }
        Ok(())
    }
}

fn write_text(text: &[u32], points: &mut Vec<u32>) {
    points.push(u32::from('"'));
    for &point in text {
        let escaped = match point {
            8 => Some("\\b"),
            9 => Some("\\t"),
            10 => Some("\\n"),
            12 => Some("\\f"),
            13 => Some("\\r"),
            34 => Some("\\\""),
            92 => Some("\\\\"),
            _ => None,
        };
        if let Some(escaped) = escaped {
            points.extend(escaped.chars().map(u32::from));
        } else if point < 32 {
            points.extend(format!("\\u{point:04x}").chars().map(u32::from));
        } else {
            points.push(point);
        }
    }
    points.push(u32::from('"'));
}

pub(crate) fn utf8(points: &[u32]) -> Result<String, Error> {
    if let Some(start) = points
        .iter()
        .position(|&point| char::from_u32(point).is_none())
    {
        let end = points
            .iter()
            .enumerate()
            .skip(start)
            .find_map(|(index, &point)| char::from_u32(point).map(|_| index))
            .unwrap_or(points.len());
        let (description, position) = if end == start + 1 {
            (
                format!("character '\\u{:04x}'", points[start]),
                format!("position {start}"),
            )
        } else {
            ("characters".into(), format!("position {start}-{}", end - 1))
        };
        return Err(Error::typed(
            "UnicodeEncodeError",
            format!(
                "'utf-8' codec can't encode {description} in {position}: surrogates not allowed"
            ),
        ));
    }
    points
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<_>>()
        .ok_or_else(|| Error::value("invalid Unicode text"))
}
