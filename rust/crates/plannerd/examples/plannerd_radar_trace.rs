use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::{car_capnp::radar_data, log_capnp::radar_state};
use openpilot_plannerd::{
    fast_radar::{FastInput, FastRadarOverlay},
    lead::Lead,
    radar::Radar,
    radar_decode,
    stopping_lead::{StopInput, StoppingLeadFilter},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    FastNew {
        id: u64,
        delay: u64,
    },
    Observe {
        id: u64,
        radar: PathBuf,
        time: u64,
        valid: bool,
    },
    Ready {
        id: u64,
        radar: PathBuf,
    },
    Build {
        id: u64,
        radar: PathBuf,
        points: PathBuf,
        speed: u64,
        radar_ns: u64,
        live_ns: u64,
        radar_valid: bool,
        live_valid: bool,
    },
    StopNew {
        id: u64,
    },
    Stop {
        id: u64,
        radar: PathBuf,
        stopping: bool,
        speed: u64,
        time: u64,
        valid: bool,
    },
}

fn lead(value: &Lead) -> Value {
    let values = [
        value.d_rel,
        value.y_rel,
        value.v_rel,
        value.a_rel,
        value.v_lead,
        value.d_path,
        value.v_lat,
        value.v_lead_k,
        value.a_lead_k,
        value.a_lead_tau,
        value.model_prob,
        value.a_lead,
        value.j_lead,
        value.score,
        value.cut_out_time,
        value.cut_out_confidence,
    ];
    json!({"floats":values.map(f64::to_bits), "status":value.status, "radar":value.radar,
        "id":value.radar_track_id, "fcw":value.fcw})
}

fn radar(value: &Radar) -> Value {
    json!([
        lead(&value.lead_one),
        lead(&value.lead_two),
        lead(&value.lead_cut_in_risk)
    ])
}

fn decode(path: &Path) -> Result<Radar, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let message = serialize::read_message(bytes.as_slice(), ReaderOptions::new())?;
    Ok(radar_decode::radar(
        message.get_root::<radar_state::Reader<'_>>()?,
    )?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let actions: Vec<Action> = serde_json::from_str(&input)?;
    let mut fast = BTreeMap::new();
    let mut stopped = BTreeMap::new();
    let mut outputs = Vec::new();
    for action in actions {
        let output = match action {
            Action::FastNew { id, delay } => {
                fast.insert(id, FastRadarOverlay::new(f64::from_bits(delay)));
                Value::Null
            }
            Action::Observe {
                id,
                radar,
                time,
                valid,
            } => {
                fast.get_mut(&id)
                    .ok_or("missing overlay")?
                    .observe(&decode(&radar)?, time, valid);
                Value::Null
            }
            Action::Ready { id, radar } => json!(fast
                .get(&id)
                .ok_or("missing overlay")?
                .lead_one_ready(&decode(&radar)?)),
            Action::Build {
                id,
                radar: path,
                points,
                speed,
                radar_ns,
                live_ns,
                radar_valid,
                live_valid,
            } => {
                let bytes = std::fs::read(points)?;
                let message = serialize::read_message(bytes.as_slice(), ReaderOptions::new())?;
                let points = radar_decode::points(message.get_root::<radar_data::Reader<'_>>()?)?;
                let result = fast.get_mut(&id).ok_or("missing overlay")?.build(
                    &decode(&path)?,
                    &points,
                    FastInput {
                        ego_speed: f64::from_bits(speed),
                        radar_mono_ns: radar_ns,
                        live_mono_ns: live_ns,
                        radar_valid,
                        live_valid,
                    },
                )?;
                json!({"radar":radar(&result.radar_state), "mask":result.lead_mask, "id":result.lead_one_track_id,
                    "age":result.selection_age_s.to_bits(), "reason":result.lead_one_reason.name()})
            }
            Action::StopNew { id } => {
                stopped.insert(id, StoppingLeadFilter::default());
                Value::Null
            }
            Action::Stop {
                id,
                radar: path,
                stopping,
                speed,
                time,
                valid,
            } => {
                let filter = stopped.get_mut(&id).ok_or("missing stop filter")?;
                let result = filter.update(
                    &decode(&path)?,
                    StopInput {
                        stopping,
                        speed: f64::from_bits(speed),
                        mono_time_ns: time,
                        valid,
                    },
                )?;
                json!({"radar":radar(&result), "mask":filter.held_mask})
            }
        };
        outputs.push(output);
    }
    serde_json::to_writer(io::stdout().lock(), &outputs)?;
    Ok(())
}
