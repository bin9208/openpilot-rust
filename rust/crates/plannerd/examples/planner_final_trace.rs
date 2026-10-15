use openpilot_plannerd::{
    braking_gap::{ComfortInput, LeadComfortState, Observation},
    coasting::{CoastingInput, CruiseCoastingPlan},
    gap_recovery::GapForecast,
    gap_recovery::{GapSample, LeadGapState},
    lane_model_speed::LaneModelSpeedGuard,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, Read};
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Action {
    Coasting {
        input: CoastingInput,
    },
    Lane {
        ego: f64,
        start: f64,
        end: f64,
    },
    Gap {
        input: GapSample,
    },
    Comfort {
        enabled: bool,
        track_id: i32,
        dt: f64,
        distance: f64,
        relative_speed: f64,
        base_tf: f64,
        speeds: [f64; 3],
        leads: [f64; 3],
        lead_accel: f64,
        gaps: [f64; 3],
        danger: [f64; 3],
        stop: f64,
    },
    Forecast {
        level: i32,
        speeds: [f64; 3],
        leads: [f64; 3],
        base_tf: f64,
        stop: f64,
    },
    Reset,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = String::new();
    io::stdin().read_to_string(&mut bytes)?;
    let mut coasting = CruiseCoastingPlan::default();
    let mut lane = LaneModelSpeedGuard::new(20);
    let mut gap = LeadGapState::default();
    let mut comfort = LeadComfortState::default();
    let mut output: Vec<Value> = Vec::new();
    for action in serde_json::from_str::<Vec<Action>>(&bytes)? {
        output.push(match action {
            Action::Coasting { input } => json!([coasting.update(&input), coasting.stable_time()]),
            Action::Lane { ego, start, end } => json!(lane.update(ego, start, end)),
            Action::Gap { input } => json!([gap.update(input), serde_json::to_value(&gap)?]),
            Action::Forecast {
                level,
                speeds,
                leads,
                base_tf,
                stop,
            } => json!(gap.margins(GapForecast {
                level,
                times: &[0., 1., 3.],
                ego_speeds: &speeds,
                lead_speeds: &leads,
                base_tf,
                stop_distance: stop,
                lead_distances: None,
                desired_distances: None
            })),
            Action::Comfort {
                enabled,
                track_id,
                dt,
                distance,
                relative_speed,
                base_tf,
                speeds,
                leads,
                lead_accel,
                gaps,
                danger,
                stop,
            } => {
                comfort.update(Observation {
                    enabled,
                    track_id,
                    dt,
                    distance,
                    relative_speed,
                });
                json!(comfort.comfort_relief(ComfortInput {
                    dt,
                    enabled,
                    base_tf,
                    ego_speeds: &speeds,
                    lead_speeds: &leads,
                    lead_accel,
                    physical_gaps: &gaps,
                    danger_margins: &danger,
                    times: &[0., 1., 3.],
                    stop_distance: stop
                }))
            }
            Action::Reset => {
                coasting.reset();
                lane = LaneModelSpeedGuard::new(20);
                gap = LeadGapState::default();
                Value::Null
            }
        });
    }
    serde_json::to_writer(io::stdout().lock(), &output)?;
    Ok(())
}
