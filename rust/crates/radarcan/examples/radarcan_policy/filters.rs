use openpilot_radarcan::{
    lead_filter::LeadFilter, numerics::Numerics, point::Point, scalar::bits, track::Track, Error,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct LeadRequest {
    #[serde(deserialize_with = "openpilot_radarcan::scalar::deserialize_float")]
    velocity: f64,
    #[serde(deserialize_with = "openpilot_radarcan::scalar::deserialize_float")]
    dt: f64,
    actions: Vec<LeadAction>,
}
#[derive(Deserialize)]
struct LeadAction {
    #[serde(deserialize_with = "openpilot_radarcan::scalar::deserialize_float")]
    velocity: f64,
    #[serde(default)]
    reset: bool,
    #[serde(default)]
    stationary: bool,
}

pub fn lead(request: Value) -> Result<Value, Error> {
    let request: LeadRequest = serde_json::from_value(request)?;
    let mut state = match LeadFilter::new(request.velocity, request.dt) {
        Ok(state) => state,
        Err(error) => {
            let kind = if matches!(error, Error::PowerOverflow) {
                "OverflowError"
            } else {
                "ValueError"
            };
            return Ok(json!({"error":{"kind":kind,"message":error.to_string()}}));
        }
    };
    let mut output = Vec::new();
    for action in request.actions {
        if action.reset {
            state.reset(action.velocity);
        } else {
            state.update(action.velocity, action.stationary)?;
        }
        let fields = serde_json::to_value(&state)?;
        output.push(json!({"bits":bits(lead_floats(&state)),"fields":fields}));
    }
    Ok(json!(output))
}

#[derive(Deserialize)]
struct TrackRequest {
    track_id: u64,
    #[serde(deserialize_with = "openpilot_radarcan::scalar::deserialize_float")]
    dt: f64,
    point: Point,
    actions: Vec<TrackAction>,
}
#[derive(Deserialize)]
struct TrackAction {
    point: serde_json::Map<String, Value>,
    #[serde(default)]
    a_ego: f64,
}

pub fn track(request: Value, numerics: &mut Numerics) -> Result<Value, Error> {
    let request: TrackRequest = serde_json::from_value(request)?;
    let mut point = request.point;
    let mut state = match Track::new(request.track_id, &point, request.dt) {
        Ok(state) => state,
        Err(error) => {
            let kind = match error {
                Error::InvalidPeriod => "ValueError",
                Error::DivisionByZero => "ZeroDivisionError",
                Error::InfiniteInteger | Error::IntegerOverflow | Error::PowerOverflow => {
                    "OverflowError"
                }
                _ => return Err(error),
            };
            return Ok(json!({"point":point,"error":{"kind":kind,"message":error.to_string()}}));
        }
    };
    let mut output = Vec::new();
    for action in request.actions {
        let mut updated = serde_json::to_value(&point)?;
        updated
            .as_object_mut()
            .ok_or(Error::Contract("point object absent"))?
            .extend(action.point);
        point = serde_json::from_value(updated)?;
        state.update(&point, action.a_ego, numerics)?;
        state.write_acceleration(&mut point);
        let fields = serde_json::to_value(&state)?;
        let track_bits = bits([
            ("dRel", state.d_rel),
            ("vRel", state.v_rel),
            ("yRel", state.y_rel),
            ("yvRel", state.yv_rel),
            ("vLead", state.v_lead),
            ("aLead", state.a_lead),
            ("jLead", state.j_lead),
            ("dt", state.dt),
        ]);
        let point_bits = bits(
            [
                ("dRel", point.d_rel),
                ("vRel", point.v_rel),
                ("yRel", point.y_rel),
                ("yvRel", point.yv_rel),
                ("vLead", point.v_lead),
                ("aLead", point.a_lead),
                ("jLead", point.j_lead),
                ("aRel", point.a_rel),
            ]
            .map(|(name, value)| (name, f64::from(value))),
        );
        output
            .push(json!({"point":point,"point_bits":point_bits,"bits":track_bits,"fields":fields}));
    }
    Ok(json!(output))
}

fn lead_floats(state: &LeadFilter) -> [(&str, f64); 16] {
    [
        ("dt", state.dt),
        ("alpha_slow", state.alpha_slow),
        ("beta_slow", state.beta_slow),
        ("alpha_range", state.alpha_range),
        ("beta_range", state.beta_range),
        ("residual_alpha", state.residual_alpha),
        ("noise_alpha", state.noise_alpha),
        ("accel_alpha_range", state.accel_alpha_range),
        ("residual_scale_squared", state.residual_scale_squared),
        ("max_accel_step", state.max_accel_step),
        ("velocity", state.velocity),
        ("acceleration", state.acceleration),
        ("mean_residual", state.mean_residual),
        ("previous_residual", state.previous_residual),
        ("residual_variance", state.residual_variance),
        ("response_weight", state.response_weight),
    ]
}
