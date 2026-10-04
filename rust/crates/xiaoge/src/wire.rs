use openpilot_cereal::{
    car_capnp::car_state,
    log_capnp::{model_data_v2, selfdrive_state},
};
use openpilot_logging::{Fields, Value};

pub fn fields<const N: usize>(values: [(&str, Value); N]) -> Fields {
    values
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

pub fn object<const N: usize>(values: [(&str, Value); N]) -> Value {
    Value::Object(fields(values))
}

pub fn compact(value: &Fields) -> Result<Vec<u8>, std::fmt::Error> {
    let text = value.to_json()?;
    let mut quoted = false;
    let mut escaped = false;
    Ok(text
        .bytes()
        .filter(|&byte| {
            if escaped {
                escaped = false;
                return true;
            }
            match byte {
                b'\\' if quoted => escaped = true,
                b'"' => quoted = !quoted,
                b' ' if !quoted => return false,
                _ => (),
            }
            true
        })
        .collect())
}

pub fn car(reader: car_state::Reader<'_>) -> Fields {
    let speed = f64::from(reader.get_v_ego());
    fields([
        ("vEgo", Value::Float(if speed < 0.0 { 0.0 } else { speed })),
        (
            "steeringAngleDeg",
            Value::Float(f64::from(reader.get_steering_angle_deg())),
        ),
        (
            "leftLatDist",
            Value::Float(f64::from(reader.get_left_lat_dist())),
        ),
        ("leftBlindspot", Value::Bool(reader.get_left_blindspot())),
        ("rightBlindspot", Value::Bool(reader.get_right_blindspot())),
    ])
}

fn first(values: capnp::primitive_list::Reader<'_, f32>) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        f64::from(values.get(0))
    }
}

pub fn model(reader: model_data_v2::Reader<'_>) -> Result<Fields, capnp::Error> {
    let leads = reader.get_leads_v3()?;
    let lead = if leads.is_empty() {
        object([
            ("x", Value::Float(0.0)),
            ("y", Value::Float(0.0)),
            ("v", Value::Float(0.0)),
            ("prob", Value::Float(0.0)),
        ])
    } else {
        let lead = leads.get(0);
        object([
            ("x", Value::Float(first(lead.get_x()?))),
            ("y", Value::Float(first(lead.get_y()?))),
            ("v", Value::Float(first(lead.get_v()?))),
            ("prob", Value::Float(f64::from(lead.get_prob()))),
        ])
    };
    let probabilities = reader.get_lane_line_probs()?;
    let lanes = if probabilities.len() >= 3 {
        [probabilities.get(1), probabilities.get(2)]
    } else {
        [0.0; 2]
    };
    let meta = reader.get_meta()?;
    let yaw = reader.get_orientation_rate()?.get_z()?;
    let mut curvature = first(yaw);
    for value in yaw.iter().skip(1).map(f64::from) {
        if value.abs() > curvature.abs() {
            curvature = value;
        }
    }
    Ok(fields([
        ("lead0", lead),
        (
            "laneLineProbs",
            Value::Array(
                lanes
                    .into_iter()
                    .map(|value| Value::Float(f64::from(value)))
                    .collect(),
            ),
        ),
        (
            "meta",
            object([
                (
                    "distanceToRoadEdgeLeft",
                    Value::Float(f64::from(meta.get_distance_to_road_edge_left())),
                ),
                (
                    "distanceToRoadEdgeRight",
                    Value::Float(f64::from(meta.get_distance_to_road_edge_right())),
                ),
            ]),
        ),
        (
            "curvature",
            object([("maxOrientationRate", Value::Float(curvature))]),
        ),
    ]))
}

pub fn system(reader: selfdrive_state::Reader<'_>) -> Fields {
    fields([
        ("enabled", Value::Bool(reader.get_enabled())),
        ("active", Value::Bool(reader.get_active())),
    ])
}

pub fn packet(
    sequence: u64,
    timestamp: f64,
    ip: &str,
    data: Fields,
) -> Result<Vec<u8>, std::fmt::Error> {
    Ok(fields([
        ("version", Value::Integer(1)),
        ("sequence", Value::Integer(i128::from(sequence))),
        ("timestamp", Value::Float(timestamp)),
        ("ip", Value::Text(ip.to_owned())),
        ("data", Value::Object(data)),
    ])
    .to_json()?
    .into_bytes())
}
