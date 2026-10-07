use super::IngressError;
use crate::sources::{Control, Instruction, Lifecycle, SafetyItem, Snapshot, Source};
use openpilot_logmessaged::{JsonValue, JsonView};

type Result<T> = std::result::Result<T, IngressError>;
fn fail(code: &'static str) -> IngressError {
    IngressError(code)
}
fn field(value: &JsonValue, name: &str, code: &'static str) -> Result<JsonValue> {
    value.get(name).ok_or(fail(code))
}
fn keys(value: &JsonValue, expected: &[&str], code: &'static str) -> Result<()> {
    let JsonView::Object(values) = value.view() else {
        return Err(fail(code));
    };
    if values.len() != expected.len()
        || values.iter().any(|(name, _)| {
            !expected
                .iter()
                .any(|e| name.iter().copied().eq(e.chars().map(u32::from)))
        })
    {
        return Err(fail(code));
    }
    Ok(())
}
fn boolean(value: &JsonValue, name: &str, code: &'static str) -> Result<bool> {
    match field(value, name, code)?.view() {
        JsonView::Bool(b) => Ok(b),
        _ => Err(fail(code)),
    }
}
fn text(value: &JsonValue, name: &str, maximum: usize, code: &'static str) -> Result<String> {
    let field = field(value, name, code)?;
    match field.view() {
        JsonView::Text(points) if points.len() <= maximum => field
            .to_utf8()
            .map(Ok)
            .unwrap_or_else(|| field.to_json().map_err(|_| fail(code))),
        _ => Err(fail(code)),
    }
}
fn number(value: &JsonValue, name: &str, range: (f64, f64), code: &'static str) -> Result<f64> {
    let field = field(value, name, code)?;
    let n = match field.view() {
        JsonView::Integer(n) => n.parse::<f64>().map_err(|_| fail(code))?,
        JsonView::Float(n) => n,
        _ => return Err(fail(code)),
    };
    if !n.is_finite() || !(range.0..=range.1).contains(&n) {
        return Err(fail(code));
    }
    Ok(n)
}
fn integer(value: &JsonValue, name: &str, range: (u64, u64), code: &'static str) -> Result<u64> {
    let field = field(value, name, code)?;
    let JsonView::Integer(n) = field.view() else {
        return Err(fail(code));
    };
    let n = n.parse::<u64>().map_err(|_| fail(code))?;
    if !(range.0..=range.1).contains(&n) {
        return Err(fail(code));
    }
    Ok(n)
}
fn instruction(item: &JsonValue, now: f64) -> Result<Instruction> {
    let present = boolean(item, "present", "guidance")?;
    keys(
        item,
        if present {
            &["present", "maneuver", "distanceM", "roadName", "mainText"]
        } else {
            &["present"]
        },
        "guidance",
    )?;
    if !present {
        return Ok(Instruction::default());
    }
    let kind = match text(item, "maneuver", 32, "guidance")?.as_str() {
        "straight" => 0,
        "left" => 12,
        "right" => 13,
        "u_turn" => 14,
        "fork_left" => 7,
        "fork_right" => 6,
        "ramp_left" => 102,
        "ramp_right" => 101,
        "roundabout" => 131,
        "arrive" => 201,
        "slight_left" => 1000,
        "slight_right" => 1001,
        _ => return Err(fail("guidance")),
    };
    Ok(Instruction {
        present,
        turn_type: kind,
        distance_m: number(item, "distanceM", (0., 2_000_000.), "guidance")?,
        road_name: text(item, "roadName", 256, "guidance")?,
        main_text: text(item, "mainText", 256, "guidance")?,
        original_main_text_json: field(item, "mainText", "guidance")?
            .to_utf8()
            .is_none()
            .then(|| {
                field(item, "mainText", "guidance")?
                    .to_json()
                    .map_err(|_| fail("guidance"))
            })
            .transpose()?,
        received_mono_s: Some(now),
        ..Instruction::default()
    })
}
fn safety(value: &JsonValue, now: f64) -> Result<(Option<SafetyItem>, Option<SafetyItem>)> {
    if !boolean(value, "present", "safety")? {
        keys(value, &["present"], "safety")?;
        return Ok((None, None));
    }
    let kind = text(value, "kind", 32, "safety")?;
    let mut expected = vec!["present", "kind", "distanceM"];
    if kind != "speed_bump" {
        expected.push("speedKph");
    }
    if value.get("revision").is_some() {
        expected.push("revision");
    }
    keys(value, &expected, "safety")?;
    let revision = value
        .get("revision")
        .map(|_| integer(value, "revision", (1, i64::MAX.unsigned_abs()), "safety"))
        .transpose()?;
    let distance = number(value, "distanceM", (0., 2_000_000.), "safety")?;
    if distance <= 0. {
        return Err(fail("safety"));
    }
    let (kind, section, reason, speed) = match kind.as_str() {
        "speed_bump" => (22, false, "bump", 0.),
        "fixed_camera" => (
            1,
            false,
            "cam",
            number(value, "speedKph", (0., 250.), "safety")?,
        ),
        "mobile_camera" => (
            7,
            false,
            "cam",
            number(value, "speedKph", (0., 250.), "safety")?,
        ),
        "section_camera" => (
            2,
            true,
            "section",
            number(value, "speedKph", (0., 250.), "safety")?,
        ),
        _ => return Err(fail("safety")),
    };
    if kind != 22 && speed <= 0. {
        return Err(fail("safety"));
    }
    let item = SafetyItem {
        kind,
        distance_m: distance,
        speed_limit_kph: speed,
        received_mono_s: now,
        reason: reason.into(),
        section,
        section_type: -1,
        block_type: -1,
        block_speed_kph: 0.,
        block_distance_m: 0.,
        revision,
    };
    Ok(if kind == 22 {
        (None, Some(item))
    } else {
        (Some(item), None)
    })
}

pub fn parse(root: &JsonValue, now: f64) -> Result<Snapshot> {
    if !root.is_object() {
        return Err(fail("payload_type"));
    }
    keys(
        root,
        &[
            "schema",
            "sessionId",
            "sequence",
            "sentMonotonicMs",
            "lifecycle",
            "guidance",
            "safety",
            "road",
            "route",
        ],
        "root_keys",
    )?;
    if text(root, "schema", 19, "schema")? != "naver.navigation.v1" {
        return Err(fail("schema"));
    }
    if !now.is_finite() || !(0. ..=1_000_000_000_000.).contains(&now) {
        return Err(fail("received_mono_s"));
    }
    let session = text(root, "sessionId", 64, "session_id")?;
    let session = uuid::Uuid::parse_str(&session)
        .map_err(|_| fail("session_id"))?
        .to_string();
    if text(root, "sessionId", 64, "session_id")?.to_lowercase() != session {
        return Err(fail("session_id"));
    }
    let sequence = integer(root, "sequence", (0, i64::MAX.unsigned_abs()), "sequence")?;
    let sent = field(root, "sentMonotonicMs", "sent_monotonic_ms")?;
    match sent.view() {
        JsonView::Integer(_) => {
            integer(root, "sentMonotonicMs", (0, u64::MAX), "sent_monotonic_ms")?;
        }
        JsonView::Float(value)
            if value.is_finite() && (0. ..18_446_744_073_709_551_616.).contains(&value) => {}
        _ => return Err(fail("sent_monotonic_ms")),
    }
    let lifecycle = match text(root, "lifecycle", 16, "lifecycle")?.as_str() {
        "idle" => Lifecycle::Idle,
        "guiding" => Lifecycle::Guiding,
        "stopped" => Lifecycle::Stopped,
        "arrived" => Lifecycle::Arrived,
        _ => return Err(fail("lifecycle")),
    };
    let guidance = field(root, "guidance", "guidance")?;
    keys(&guidance, &["current", "next"], "guidance")?;
    let current = instruction(&field(&guidance, "current", "guidance")?, now)?;
    let next = instruction(&field(&guidance, "next", "guidance")?, now)?;
    let (safety, secondary_safety) = safety(&field(root, "safety", "safety")?, now)?;
    let road = field(root, "road", "road")?;
    let valid_limit = boolean(&road, "limitValid", "road")?;
    let valid_category = boolean(&road, "categoryValid", "road")?;
    let mut expected = vec!["limitValid", "categoryValid"];
    if valid_limit {
        expected.push("limitKph");
    }
    if valid_category {
        expected.push("category");
    }
    keys(&road, &expected, "road")?;
    let road_limit = valid_limit
        .then(|| number(&road, "limitKph", (0., 250.), "road"))
        .transpose()?;
    let road_category = valid_category
        .then(|| integer(&road, "category", (0, 2_147_483_647), "road"))
        .transpose()?
        .map(|v| i64::try_from(v).unwrap_or(0));
    let mut control = Control {
        current,
        next,
        safety,
        secondary_safety,
        road_limit_kph: road_limit,
        road_limit_received_mono_s: road_limit.map(|_| now),
        road_category,
        road_category_received_mono_s: road_category.map(|_| now),
        ..Control::default()
    };
    parse_route(&field(root, "route", "route")?, now, &mut control)?;
    if lifecycle != Lifecycle::Guiding
        && (control.current.present
            || control.next.present
            || control.safety.is_some()
            || control.secondary_safety.is_some()
            || control.road_limit_kph.is_some()
            || control.road_category.is_some()
            || control.route_present)
    {
        return Err(fail("lifecycle_consistency"));
    }
    Ok(Snapshot {
        source: Source::NaverV1,
        session_id: session,
        sequence,
        lifecycle,
        received_mono_s: now,
        activation_epoch: 0,
        control,
        owner_received_mono_s: None,
        original_json: has_non_utf8(root)
            .then(|| root.to_json().map_err(|_| fail("guidance")))
            .transpose()?,
    })
}
fn has_non_utf8(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Text(_) => value.to_utf8().is_none(),
        JsonView::Object(fields) => fields.iter().any(|(_, value)| has_non_utf8(value)),
        JsonView::Array(values) => values.iter().any(has_non_utf8),
        _ => false,
    }
}

fn parse_route(route: &JsonValue, now: f64, c: &mut Control) -> Result<()> {
    if !boolean(route, "present", "route")? {
        keys(route, &["present"], "route")?;
        return Ok(());
    }
    let destination = boolean(route, "destinationValid", "route")?;
    let mut expected = vec![
        "present",
        "remainingDistanceM",
        "remainingTimeSec",
        "offRoute",
        "destinationValid",
    ];
    if destination {
        expected.extend(["destinationLatitude", "destinationLongitude"]);
    }
    if route.get("points").is_some() {
        expected.push("points");
    }
    if route.get("revision").is_some() {
        expected.push("revision");
    }
    keys(route, &expected, "route")?;
    c.route_present = true;
    c.route_received_mono_s = Some(now);
    c.route_revision = route
        .get("revision")
        .map(|_| integer(route, "revision", (0, i64::MAX.unsigned_abs()), "route"))
        .transpose()?;
    c.remaining_distance_m = number(route, "remainingDistanceM", (0., 2_000_000.), "route")?;
    c.remaining_time_s = number(route, "remainingTimeSec", (0., 604_800.), "route")?;
    c.off_route = boolean(route, "offRoute", "route")?;
    if destination {
        c.destination = Some((
            number(route, "destinationLatitude", (-90., 90.), "route")?,
            number(route, "destinationLongitude", (-180., 180.), "route")?,
        ));
        c.destination_present = true;
        c.destination_received_mono_s = Some(now);
    }
    if let Some(points) = route.get("points") {
        let JsonView::Array(points) = points.view() else {
            return Err(fail("route"));
        };
        if points.len() > 4096 {
            return Err(fail("route"));
        }
        for point in points {
            let JsonView::Array(values) = point.view() else {
                return Err(fail("route"));
            };
            if values.len() != 2 {
                return Err(fail("route"));
            }
            let coordinate = |v: &JsonValue, minimum: f64, maximum: f64| -> Result<f64> {
                let number = match v.view() {
                    JsonView::Integer(s) => s.parse().map_err(|_| fail("route"))?,
                    JsonView::Float(n) => n,
                    _ => return Err(fail("route")),
                };
                if !number.is_finite() || !(minimum..=maximum).contains(&number) {
                    return Err(fail("route"));
                }
                Ok(number)
            };
            c.route_points.push((
                coordinate(&values[0], -90., 90.)?,
                coordinate(&values[1], -180., 180.)?,
            ));
        }
    }
    Ok(())
}
