use openpilot_jetlink::{
    adapter::Adapter,
    contract,
    owner::GadgetOwner,
    transition::{ControlState, Mode, Outcome, Transition},
    wire, Deadline,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{self, BufRead},
    time::Duration,
};
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Transition {
        previously_active: bool,
        steps: Vec<Step>,
    },
    Validate {
        raw: Option<String>,
        identity: contract::Identity,
    },
    Prepare {
        steps: Vec<Prepare>,
    },
    Parse {
        values: Vec<f32>,
    },
    Descriptors,
    Wire {
        kind: u16,
        sequence: u32,
        payload: Vec<u8>,
        gadget: bool,
    },
    Owner {
        steps: Vec<Ownership>,
    },
}
#[derive(Deserialize)]
struct Step {
    mode: Mode,
    ready: bool,
    validated: bool,
    controls: ControlState,
    outcome: Outcome,
}
#[derive(Deserialize)]
struct Prepare {
    desire: [f32; 8],
    traffic: [f32; 2],
    action: [f32; 2],
    prepare_only: bool,
}
#[derive(Deserialize)]
struct Ownership {
    enable: bool,
    offroad: bool,
    egpu: bool,
    fail: bool,
}
fn run(request: Request) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(match request {
        Request::Transition {
            previously_active,
            steps,
        } => {
            let mut state = Transition::new(previously_active);
            json!(steps
                .into_iter()
                .map(|s| state.update(s.mode, s.ready, s.validated, s.controls, s.outcome))
                .collect::<Vec<_>>())
        }
        Request::Validate { raw, identity } => json!(contract::validation_matches(
            raw.as_deref().map(str::as_bytes),
            &identity
        )),
        Request::Prepare { steps } => {
            let mut adapter = Adapter::new()?;
            let warped = vec![0; contract::WARPED_BYTES];
            let mut output = Vec::new();
            for step in steps {
                let packed = adapter.prepare(
                    &warped,
                    step.desire,
                    step.traffic,
                    step.action,
                    step.prepare_only,
                )?;
                output.push(json!({"packed":packed,"reset":adapter.reset_next()}));
            }
            json!(output)
        }
        Request::Parse { values } => {
            let p = Adapter::new()?.parse(&values, Deadline::after(Duration::from_secs(3))?)?;
            json!({"plan":p.plan.as_flattened(), "plan_stds":p.plan_std.as_flattened(), "pose":p.pose.mean,"pose_stds":p.pose.std,
                "wide_from_device_euler":p.wide_euler.mean,"wide_from_device_euler_stds":p.wide_euler.std,"road_transform":p.road_transform.mean,"road_transform_stds":p.road_transform.std,
                "lane_lines":p.lanes.mean.as_slice(),"lane_lines_stds":p.lanes.std.as_slice(),"road_edges":p.edges.mean.as_slice(),"road_edges_stds":p.edges.std.as_slice(),
                "lead":p.leads.mean.as_slice(),"lead_stds":p.leads.std.as_slice(),"lane_lines_prob":p.lane_prob,"lead_prob":p.lead_prob,"meta":p.meta.as_slice(),"desire_state":p.desire_state,"desire_pred":p.desire_prediction,"action":p.direct_action})
        }
        Request::Descriptors => {
            json!({"descriptors":wire::descriptors(),"strings":wire::strings()})
        }
        Request::Wire {
            kind,
            sequence,
            payload,
            gadget,
        } => json!(wire::frame(kind, sequence, &payload, gadget)?),
        Request::Owner { steps } => {
            let mut owner = GadgetOwner::default();
            let mut calls = 0;
            let mut output = Vec::new();
            for step in steps {
                let callback = || {
                    calls += 1;
                    if step.fail {
                        Err(openpilot_jetlink::Error::Closed)
                    } else {
                        Ok(())
                    }
                };
                let result = if step.enable {
                    owner.enable(step.offroad, step.egpu, callback)
                } else {
                    owner.disable(step.offroad, callback)
                };
                output.push(json!({"ok":result.is_ok(),"enabled":owner.enabled(),"calls":calls}));
            }
            json!(output)
        }
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        println!("{}", run(serde_json::from_str(&line?)?)?);
    }
    Ok(())
}
