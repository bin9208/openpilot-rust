use super::packet::{float, truth};
use serde_json::Value;

pub fn point(value: &Value) -> Option<(f64, f64)> {
    let (lon, lat) = match value {
        Value::Object(map) => {
            if map.get("valid").is_some_and(|v| !truth(v)) {
                return None;
            }
            let lon = map
                .get("x")
                .filter(|v| !v.is_null())
                .or_else(|| map.get("lon").or_else(|| map.get("longitude")))?;
            let lat = map
                .get("y")
                .filter(|v| !v.is_null())
                .or_else(|| map.get("lat").or_else(|| map.get("latitude")))?;
            (float(lon)?, float(lat)?)
        }
        Value::Array(values) if values.len() >= 2 => (float(&values[0])?, float(&values[1])?),
        _ => return None,
    };
    (lon.is_finite()
        && lat.is_finite()
        && (-180. ..=180.).contains(&lon)
        && (-90. ..=90.).contains(&lat))
    .then_some((lon, lat))
}

pub fn route_points(value: &Value, depth: usize) -> Option<Vec<(f64, f64)>> {
    if value.is_null() {
        return Some(Vec::new());
    }
    if depth > 8 {
        return None;
    }
    match value {
        Value::Object(_) => {
            for key in [
                "vrtx",
                "vertices",
                "vertexes",
                "coordinates",
                "coords",
                "points",
                "path",
                "route",
            ] {
                if let Some(value) = value.get(key).filter(|v| !v.is_null()) {
                    return route_points(value, depth + 1);
                }
            }
            point(value).map(|p| vec![p])
        }
        Value::Array(points) => Some(points.iter().filter_map(point).collect()),
        _ => None,
    }
}

pub fn summary_payload(value: &Value, depth: usize) -> Option<&Value> {
    if value.is_null() || depth > 8 {
        return None;
    }
    if value.is_object() {
        for key in [
            "vrtx",
            "vertices",
            "vertexes",
            "coordinates",
            "coords",
            "points",
            "path",
            "route",
        ] {
            if let Some(value) = value.get(key).filter(|v| !v.is_null()) {
                return summary_payload(value, depth + 1);
            }
        }
    }
    Some(value)
}

pub fn route_summary(value: &Value) -> Value {
    let mut points = Vec::new();
    if let Some(value) = summary_payload(value, 0) {
        if let Value::Array(values) = value {
            for value in values {
                if let Some(point) = point(value) {
                    points.push(point);
                    if points.len() == 20_000 {
                        break;
                    }
                }
            }
        } else if let Some(point) = point(value) {
            points.push(point);
        }
    }
    let mut summary = serde_json::json!({"pointCount":points.len()});
    if let Some(point) = points.first() {
        summary["first"] = serde_json::json!({"lon":point.0,"lat":point.1});
    }
    if let Some(point) = points.last() {
        summary["last"] = serde_json::json!({"lon":point.0,"lat":point.1});
    }
    if points.len() == 20_000 {
        summary["truncated"] = true.into();
    }
    summary
}
