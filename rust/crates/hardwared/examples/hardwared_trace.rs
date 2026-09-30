use openpilot_hardwared::{
    fan::FanController,
    policy::{Input, Policy},
    power::{PowerMonitoring, Shutdown},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead};
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Case {
    Fan {
        device: String,
        steps: Vec<(f64, bool)>,
    },
    Policy {
        device: String,
        steps: Vec<Input>,
    },
    Power {
        capacity: f64,
        steps: Vec<PowerStep>,
    },
}
#[derive(Deserialize)]
struct PowerStep {
    now: f64,
    voltage: Option<f64>,
    ignition: bool,
    power: f64,
    shutdown: Shutdown,
}
fn run(case: Case) -> Value {
    match case {
        Case::Fan { device, steps } => {
            let mut fan = FanController::new(2, &device);
            json!(steps
                .into_iter()
                .map(|(temp, ignition)| fan.update(temp, ignition))
                .collect::<Vec<_>>())
        }
        Case::Policy { device, steps } => {
            let mut state = Policy::new(&device);
            let mut result = Vec::new();
            for step in steps {
                let (tick, _) = state.poll(&step);
                if tick {
                    let output = state.step(&step);
                    result.push(json!({"started":output.started,"startedMonoTime":(output.started_ts.unwrap_or(0.) * 1e9) as u64,
                        "thermalStatus":output.thermal,"maxTempC":output.max_temperature as f32,"fanSpeedPercentDesired":output.fan,
                        "power_save":output.power_save,"temperature_alert":output.temperature_alert,
                        "startup":state.startup,"onroad":state.onroad}));
                }
            }
            json!(result)
        }
        Case::Power { capacity, steps } => {
            let mut state = PowerMonitoring::new(capacity);
            let mut result = Vec::new();
            for step in steps {
                let (save, error) =
                    state.calculate(step.now, step.voltage, step.ignition, step.power);
                result.push(json!({"state":state,"save":save,"error":error,"shutdown":state.should_shutdown(&step.shutdown)}));
            }
            json!(result)
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        println!("{}", run(serde_json::from_str(&line?)?));
    }
    Ok(())
}
