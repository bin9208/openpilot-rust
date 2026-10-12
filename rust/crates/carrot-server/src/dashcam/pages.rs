use super::{cache::Service, paths, Failure};
use crate::{Error, Value};
use num_traits::ToPrimitive;

pub(super) fn segments(entry: &Value) -> Result<&[Value], Failure> {
    match entry.get("segmentFolders") {
        Value::Array(values) => Ok(values),
        value if !value.truth() => Ok(&[]),
        value => {
            Err(Error::Source(format!("'{}' object is not iterable", value.type_name())).into())
        }
    }
}
fn route_page(
    service: &Service,
    entry: &Value,
    offset: usize,
    limit: usize,
    descending: bool,
) -> Result<Value, Failure> {
    let all = segments(entry)?;
    let total = all.len();
    let offset = offset.min(total);
    let limit = limit.clamp(1, 2000);
    let end = offset.saturating_add(limit).min(total);
    let (begin, stop) = if descending {
        (total.saturating_sub(end), total.saturating_sub(offset))
    } else {
        (offset, end)
    };
    let page = &all[begin..stop];
    let seed = if begin > 0 {
        &all[begin - 1]
    } else {
        &Value::Null
    };
    let times = service.catalog.compute_segment_times(page, seed)?;
    let mut page = page.to_vec();
    if descending {
        page.reverse();
    }
    let mut ordered_times = Vec::new();
    for segment in &page {
        let key = crate::json_fields::key(segment)?;
        if let Value::Object(fields) = &times {
            if let Some(value) = crate::json_fields::field(fields, &key) {
                ordered_times.push((key, value.clone()));
            }
        }
    }
    let (start, stop) = service.catalog.route_time_bounds(all)?;
    let count = if entry.get("segmentCount").truth() {
        entry.get("segmentCount").int().map_err(Error::from)?
    } else {
        total.into()
    };
    let mut result = entry.clone();
    for (name, value) in [
        ("segmentFolders", Value::Array(page)),
        ("segmentTimes", Value::Object(ordered_times)),
        ("segmentCount", Value::Integer(count)),
        ("segmentOffset", Value::integer(offset)),
        ("segmentLimit", Value::integer(limit)),
        (
            "segmentsNextOffset",
            if end < total {
                Value::integer(end)
            } else {
                Value::Null
            },
        ),
        ("segmentsHasMore", Value::Bool(end < total)),
        ("routeStartEpoch", Value::integer(start)),
        ("routeEndEpoch", Value::integer(stop)),
        ("latestModifiedEpoch", Value::integer(stop)),
        (
            "latestModifiedLabel",
            Value::text(&paths::relative_time(stop, service.wall())),
        ),
    ] {
        crate::json_fields::set(&mut result, name, value)?;
    }
    Ok(result)
}
pub(super) fn routes_payload(
    service: &Service,
    offset: usize,
    limit: usize,
    segment_limit: usize,
    descending: bool,
) -> Result<Value, Failure> {
    let routes = service.visible_routes()?;
    let total = routes.len();
    let end = offset.saturating_add(limit).min(total);
    let page = routes[offset.min(total)..end]
        .iter()
        .map(|entry| route_page(service, entry, 0, segment_limit, descending))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::object([
        ("ok", Value::Bool(true)),
        ("routes", Value::Array(page)),
        ("root", super::paths::path_value(&service.root)),
        ("offset", Value::integer(offset)),
        ("limit", Value::integer(limit)),
        ("segmentLimit", Value::integer(segment_limit)),
        ("total", Value::integer(total)),
        (
            "nextOffset",
            if end < total {
                Value::integer(end)
            } else {
                Value::Null
            },
        ),
        ("hasMore", Value::Bool(end < total)),
    ]))
}
pub(super) fn segments_payload(
    service: &Service,
    route: &str,
    offset: usize,
    limit: usize,
    descending: bool,
) -> Result<Option<Value>, Failure> {
    let routes = service.visible_routes()?;
    if route.is_empty() || route.contains(['/', '\\']) || matches!(route, "." | "..") {
        return Ok(None);
    }
    let Some(entry) = routes
        .iter()
        .find(|entry| entry.get("route").text_eq(route))
    else {
        return Ok(None);
    };
    let page = route_page(service, entry, offset, limit, descending)?;
    Ok(Some(Value::object([
        ("ok", Value::Bool(true)),
        ("route", Value::text(route)),
        ("segments", page.get("segmentFolders").clone()),
        ("segmentTimes", page.get("segmentTimes").clone()),
        ("offset", page.get("segmentOffset").clone()),
        ("limit", page.get("segmentLimit").clone()),
        ("total", page.get("segmentCount").clone()),
        ("nextOffset", page.get("segmentsNextOffset").clone()),
        ("hasMore", page.get("segmentsHasMore").clone()),
    ])))
}
pub(super) fn bounded(value: Option<String>, name: &str, default: usize, maximum: usize) -> usize {
    let minimum: usize = if name == "offset" { 0 } else { 1 };
    let number = value
        .filter(|value| !value.is_empty())
        .and_then(|value| Value::text(&value).int().ok())
        .unwrap_or_else(|| default.into());
    number
        .max(minimum.into())
        .min(maximum.into())
        .to_usize()
        .unwrap_or(minimum)
}
