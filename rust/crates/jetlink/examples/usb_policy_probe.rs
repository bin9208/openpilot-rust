//! CI-only adapter for comparing native policy against pinned upstream source.
use openpilot_runtime_core::{
    jetlink_port::{Input, Port},
    jetlink_power::{Action, Draw},
};
use serde_json::{json, Value};
use std::io::{self, Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Vec<Value>> = serde_json::from_str(&input)?;
    let mut output = Vec::new();
    for case in cases {
        let mut port = Port::default();
        let mut draw = Draw::default();
        let mut rows = Vec::new();
        for step in case {
            let mut calls = Vec::<String>::new();
            let success = step["success"].as_bool().unwrap_or(true);
            let text = |key: &str| step[key].as_str();
            let flag = |key: &str| step[key].as_bool().unwrap_or(false);
            match text("op").unwrap_or("port") {
                "draw" => {
                    let _ = draw.update(flag("wanted"), text("role"), |action| {
                        calls.push(
                            if action == Action::Cut {
                                "draw-off"
                            } else {
                                "draw-on"
                            }
                            .into(),
                        );
                        if success {
                            Ok(())
                        } else {
                            Err(io::Error::other("fixture failure"))
                        }
                    });
                }
                "off" => port.off(|action| {
                    calls.push(action.name().into());
                    success
                }),
                _ => port.update(
                    Input {
                        now: step["now"].as_f64().unwrap_or(0.),
                        role: text("role"),
                        data: text("data"),
                        configured: flag("configured"),
                        host_attached: flag("host_attached"),
                        egpu: flag("egpu"),
                        udc_mode: text("udc_mode").unwrap_or(""),
                        charge: flag("charge"),
                        contract: text("contract").unwrap_or(""),
                        hottest_c: step["hottest_c"].as_f64(),
                    },
                    |action| {
                        calls.push(action.name().into());
                        success
                    },
                ),
            }
            rows.push(json!({"calls": calls, "cut": draw.cut, "draw_for": draw.settled,
                "cleared": port.cleared, "deferred": port.deferred, "held": port.held, "settled": port.settled,
                "role": port.role, "role_since": port.role_since, "roles": port.roles, "roles_since": port.roles_since,
                "plugged": port.plugged, "asks": port.asks, "next_ask": port.next_ask, "next_start": port.next_start,
                "charge_asked": port.charge_asked, "charging": port.charging, "charge_since": port.charge_since,
                "said_hot": port.said_hot, "temp": port.temperature}));
        }
        output.push(rows);
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
