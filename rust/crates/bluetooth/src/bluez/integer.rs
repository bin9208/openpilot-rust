use super::policy::decimal;
use num_traits::ToPrimitive;
use openpilot_logmessaged::{JsonValue, JsonView};

pub(super) enum Unsigned {
    Value(u32),
    OutOfRange,
    Invalid,
}

pub(super) fn convert(value: &JsonValue) -> Unsigned {
    match value.view() {
        JsonView::Bool(value) => Unsigned::Value(u32::from(value)),
        JsonView::Integer(value) => text(&value.chars().map(u32::from).collect::<Vec<_>>()),
        JsonView::Float(value) if !value.is_finite() => Unsigned::Invalid,
        JsonView::Float(value) => value
            .trunc()
            .to_u32()
            .map_or(Unsigned::OutOfRange, Unsigned::Value),
        JsonView::Text(points) => text(points),
        JsonView::Null | JsonView::Array(_) | JsonView::Object(_) => Unsigned::Invalid,
    }
}

fn space(point: u32) -> bool {
    matches!(point, 9..=13 | 32)
        || (point > 127 && char::from_u32(point).is_some_and(char::is_whitespace))
}

fn text(mut points: &[u32]) -> Unsigned {
    while points.first().is_some_and(|point| space(*point)) {
        points = &points[1..];
    }
    while points.last().is_some_and(|point| space(*point)) {
        points = &points[..points.len() - 1];
    }
    let negative = points.first() == Some(&45);
    if matches!(points.first(), Some(43 | 45)) {
        points = &points[1..];
    }
    let mut count = 0_u32;
    let mut number = Some(0_u32);
    let mut previous_digit = false;
    let mut nonzero = false;
    for point in points {
        if *point == 95 && previous_digit {
            previous_digit = false;
            continue;
        }
        let Some(digit) = decimal(*point) else {
            return Unsigned::Invalid;
        };
        count += 1;
        if count > 4300 {
            return Unsigned::Invalid;
        }
        number = number
            .and_then(|value| value.checked_mul(10))
            .and_then(|value| value.checked_add(digit));
        nonzero |= digit != 0;
        previous_digit = true;
    }
    if !previous_digit {
        return Unsigned::Invalid;
    }
    if negative && nonzero {
        return Unsigned::OutOfRange;
    }
    number.map_or(Unsigned::OutOfRange, Unsigned::Value)
}
