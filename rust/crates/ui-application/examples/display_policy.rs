use openpilot_ui_application::{radar_display, vision_status::blindspot};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Case {
    Signal {
        valid: bool,
        alive: bool,
        suspended: bool,
        color: u8,
        action: u8,
    },
    Clock {
        times: Vec<f64>,
        retention: f64,
    },
    Radar {
        input: radar_display::Input,
        show: i32,
        now: f64,
        started_frame: i64,
    },
    Blindspot {
        payload: Vec<u8>,
        now: i64,
        valid: bool,
        split: bool,
        merged: [bool; 2],
        sources: [[bool; 2]; 2],
    },
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<Case> = serde_json::from_reader(std::io::stdin())?;
    let mut output = Vec::new();
    for case in cases {
        output.push(match case {
            Case::Signal { valid,alive,suspended,color,action } => serde_json::to_value(openpilot_ui_application::signal_status::Status::new(valid,alive,suspended,color,action))?,
            Case::Clock { times,retention } => {
                let mut clock = openpilot_ui_application::display_clock::DisplayClock::default();
                serde_json::to_value(times.iter().map(|time| { clock.tick(*time); [clock.frames,clock.retention(retention)] }).collect::<Vec<_>>())?
            }
            Case::Radar { input,show,now,started_frame } => serde_json::to_value(radar_display::select(&input,show,now,started_frame))?,
            Case::Blindspot { payload,now,valid,split,merged,sources } => {
                let packet = blindspot::Sources::parse(&payload);
                let accepted = packet.is_ok();
                let fresh = packet.as_ref().is_ok_and(|p| p.fresh(i128::from(now)));
                let mut message = capnp::message::Builder::new_default();
                let mut car = message.init_root::<openpilot_cereal::car_capnp::car_state::Builder>();
                car.set_blindspot_split_sources_valid(split);
                car.set_left_blindspot(merged[0]); car.set_right_blindspot(merged[1]);
                car.set_left_blindspot_oem(sources[0][0]); car.set_left_blindspot_onnx(sources[0][1]);
                car.set_right_blindspot_oem(sources[1][0]); car.set_right_blindspot_onnx(sources[1][1]);
                let packet = packet.ok().filter(|_| fresh && valid);
                let sides = blindspot::sides(car.into_reader(),packet.as_ref());
                serde_json::json!({"accepted":accepted,"fresh":fresh,"sides":sides,"tints":sides.map(blindspot::tint)})
            }
        });
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
