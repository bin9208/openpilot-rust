use openpilot_radarcan::{base::Base, data::Data, numerics::Numerics, point::Point, scalar, Error};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};

fn rounded<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    Ok(scalar::deserialize_float(deserializer)? as f32)
}

#[derive(Deserialize)]
struct Request {
    #[serde(deserialize_with = "rounded")]
    delay: f32,
    #[serde(deserialize_with = "rounded")]
    period: f32,
    actions: Vec<Action>,
}
#[derive(Deserialize)]
struct Action {
    v_ego: f64,
    a_ego: f64,
    time: f64,
    #[serde(default)]
    fallback: bool,
    #[serde(default)]
    none: bool,
    #[serde(default)]
    points: Vec<(u64, serde_json::Map<String, Value>)>,
    #[serde(default)]
    delete: Vec<u64>,
    selected: Option<Vec<u64>>,
}

fn python_error(error: &Error) -> Value {
    let kind = match error {
        Error::InvalidPeriod | Error::NanInteger | Error::NegativeHistory => "ValueError",
        Error::InfiniteInteger | Error::IntegerOverflow | Error::PowerOverflow => "OverflowError",
        Error::EmptyHistory => "IndexError",
        Error::DivisionByZero => "ZeroDivisionError",
        _ => "NativeBoundaryError",
    };
    json!({"kind":kind,"message":error.to_string()})
}

pub fn number(value: f64) -> Value {
    if value.is_nan() {
        json!("NaN")
    } else if value == f64::INFINITY {
        json!("Infinity")
    } else if value == f64::NEG_INFINITY {
        json!("-Infinity")
    } else {
        json!(value)
    }
}

pub fn snapshot(base: &Base) -> (Value, Value) {
    let bits = scalar::bits(
        [("v_ego", base.v_ego), ("a_ego", base.a_ego)]
            .into_iter()
            .chain(base.dt.map(|dt| ("dt", dt)))
            .chain(base.last_timestamp.map(|time| ("last_timestamp", time))),
    );
    let state = json!({"frame":base.frame,"v_ego_hist":base.v_ego_hist.iter().copied().map(number).collect::<Vec<_>>(),
        "a_ego_hist":base.a_ego_hist.iter().copied().map(number).collect::<Vec<_>>(),
        "v_ego":number(base.v_ego),"a_ego":number(base.a_ego),"last_timestamp":base.last_timestamp.map(number),
        "dt":base.dt.map(number),"init_samples":base.init_samples.iter().copied().map(number).collect::<Vec<_>>(),
        "init_done":base.init_done,"pts":base.pts,"pts_order":base.pts.keys().collect::<Vec<_>>(),"tracks":base.tracks,
        "tracks_order":base.tracks.keys().collect::<Vec<_>>(),"history_maxlen":base.history_maxlen});
    (state, json!(bits))
}

pub fn trace(request: Value, numerics: &mut Numerics, stdout: &mut String) -> Result<Value, Error> {
    let request: Request = serde_json::from_value(request)?;
    let mut base = match Base::new(request.delay, request.period) {
        Ok(base) => base,
        Err(error) => return Ok(json!({"constructor_error":python_error(&error)})),
    };
    let mut output = Vec::new();
    for action in request.actions {
        let step = (|| -> Result<Option<Data>, Error> {
            base.push_ego(action.v_ego, action.a_ego)?;
            let result = if action.fallback {
                base.fallback()
            } else {
                for address in action.delete {
                    base.pts.shift_remove(&address);
                }
                for (address, values) in action.points {
                    let point = base.pts.entry(address).or_insert_with(Point::default);
                    let mut value = serde_json::to_value(&*point)?;
                    value
                        .as_object_mut()
                        .ok_or(Error::Contract("point absent"))?
                        .extend(values);
                    *point = serde_json::from_value(value)?;
                }
                if action.none {
                    None
                } else {
                    let selected = action
                        .selected
                        .unwrap_or_else(|| base.pts.keys().copied().collect());
                    Some(Data {
                        points: selected
                            .into_iter()
                            .map(|address| base.pts[&address].clone())
                            .collect(),
                        ..Data::default()
                    })
                }
            };
            base.finish(result, action.time, numerics, &mut |line| {
                stdout.push_str(line)
            })
        })();
        let (result, error) = match step {
            Ok(result) => (result, Value::Null),
            Err(error) => (None, python_error(&error)),
        };
        let (state, bits) = snapshot(&base);
        output.push(json!({"result":result,"error":error,"state":state,"bits":bits}));
    }
    Ok(json!(output))
}
