use super::{base, decoder_hyundai, decoder_parser, decoder_settings};
use openpilot_can::Packet;
use openpilot_radarcan::{
    databases::Databases,
    decoder::{Config, Interface, Kind},
    numerics::Numerics,
    Error,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Request {
    #[serde(flatten)]
    config: Config,
    constructor_ns: u64,
    actions: Vec<Action>,
    #[serde(default)]
    params: Option<std::collections::BTreeMap<String, String>>,
}
#[derive(Deserialize)]
struct Action {
    v_ego: f64,
    a_ego: f64,
    time: f64,
    packets: Vec<Packet>,
}

pub fn trace(
    request: Value,
    databases: &mut Databases,
    numerics: &mut Numerics,
    stdout: &mut String,
) -> Result<Value, Error> {
    let request: Request = serde_json::from_value(request)?;
    let mut settings = decoder_settings::Observed {
        values: request.params,
        ..Default::default()
    };
    let mut state = Interface::with_settings(
        request.config,
        &mut openpilot_radarcan::decoder::hyundai::Environment {
            databases,
            clock: &mut || request.constructor_ns,
            emit: &mut |line: &str| stdout.push_str(line),
            settings: &mut settings,
        },
    )?;
    let mut output = Vec::new();
    for action in request.actions {
        let result = state.update_carrot(
            action.v_ego,
            action.a_ego,
            action.time,
            &action.packets,
            numerics,
            &mut |line| stdout.push_str(line),
        )?;
        let (snapshot, warnings) = snapshot(&mut state)?;
        output.push(json!({"result":result,"state":snapshot,"warnings":warnings}));
    }
    Ok(if settings.values.is_some() {
        json!({"steps":output,"parameter_reads":settings.reads})
    } else {
        json!(output)
    })
}

pub fn snapshot(state: &mut Interface) -> Result<(Value, Vec<String>), Error> {
    let (mut common, bits) = base::snapshot(&state.base);
    if matches!(state.kind, Kind::Gm | Kind::Volkswagen(_)) {
        common["pts"] = Value::Object(
            state
                .base
                .pts
                .iter()
                .map(|(id, point)| Ok((format!("{id}.0"), serde_json::to_value(point)?)))
                .collect::<Result<serde_json::Map<String, Value>, serde_json::Error>>()?,
        );
    }
    let mut backend = match &state.kind {
        Kind::Honda(s) => {
            json!({"track_id":s.track_id,"radar_fault":s.radar_fault,"radar_wrong_config":s.radar_wrong_config})
        }
        Kind::Hyundai(_) => json!({}),
        Kind::Rivian(s) => json!({"track_id":s.track_id}),
        Kind::Tesla(s) => json!({"track_id":s.track_id}),
        Kind::Toyota(s) => json!({"track_id":s.track_id,"valid_cnt":s.valid_cnt}),
        Kind::Ford(s) => {
            let mut value = json!({"track_id":s.track_id,"points":s.points,"clusters":s.clusters,
                "scan_index_invalid_cnt":s.scan_index_invalid_cnt,"radar_unavailable_cnt":s.radar_unavailable_cnt,
                "prev_headerScanIndex":s.prev_header_scan_index});
            if let Some(counts) = &s.valid_cnt {
                value["valid_cnt"] = json!(counts);
            }
            value
        }
        Kind::Volkswagen(s) => json!({"_track_id_counter":s.track_id_counter,
            "_yv_state":s.yv_state.iter().map(|(id,state)| (format!("{id}.0"),state)).collect::<std::collections::BTreeMap<_,_>>()}),
        Kind::Chrysler | Kind::Gm | Kind::Fallback => json!({}),
    };
    let (parser, warnings) = match &mut state.kind {
        Kind::Hyundai(hyundai) => {
            let (value, warnings) = decoder_hyundai::snapshot(hyundai)?;
            backend = value;
            (Value::Null, warnings)
        }
        _ => {
            backend["updated_messages"] = json!(state.updated.iter().collect::<Vec<_>>());
            decoder_parser::snapshot(state.reader.as_mut())?
        }
    };
    Ok((
        json!({"common":common,"bits":bits,"backend":backend,"parser":parser}),
        warnings,
    ))
}
