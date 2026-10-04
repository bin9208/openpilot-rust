use capnp::message::Builder;
use openpilot_card::xiaoge;
use openpilot_cereal::car_capnp::car_state;
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Deserialize)]
struct Case {
    bytes: Vec<u8>,
    lanes: [i16; 2],
    spots: [bool; 2],
    now: u64,
}
#[derive(Serialize)]
struct Output {
    accepted: bool,
    applied: bool,
    lanes: [i16; 2],
    spots: [bool; 2],
}

fn trace(case: Case) -> Result<Output, capnp::Error> {
    let result = xiaoge::parse(&case.bytes);
    let mut message = Builder::new_default();
    let mut state = message.init_root::<car_state::Builder>();
    state.set_left_lane_line(case.lanes[0]);
    state.set_right_lane_line(case.lanes[1]);
    state.set_left_blindspot(case.spots[0]);
    state.set_right_blindspot(case.spots[1]);
    let applied = xiaoge::apply(state, result.as_ref().ok(), case.now);
    let state = message.get_root_as_reader::<car_state::Reader>()?;
    Ok(Output {
        accepted: result.is_ok(),
        applied,
        lanes: [state.get_left_lane_line(), state.get_right_lane_line()],
        spots: [state.get_left_blindspot(), state.get_right_blindspot()],
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let output = cases
        .into_iter()
        .map(trace)
        .collect::<Result<Vec<_>, _>>()?;
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    std::fs::write(path, serde_json::to_vec(&output)?)?;
    Ok(())
}
