use openpilot_messaging::{
    frequency::{self, FrequencyTracker},
    state::{Error, Options, State},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    error::Error as StdError,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
};

#[derive(Deserialize)]
struct Configuration {
    services: Vec<String>,
    options: Options,
}
#[derive(Deserialize)]
struct Frame {
    configuration: Option<Configuration>,
    time: f64,
    messages: Vec<Vec<u8>>,
    checks: Vec<Vec<String>>,
}

fn tracker(tracker: &FrequencyTracker) -> Value {
    json!({"min":tracker.min_frequency,"max":tracker.max_frequency,"previous":tracker.previous_time,
        "count":tracker.average.count,"index":tracker.average.index,"sum":tracker.average.sum,
        "recent_count":tracker.recent.count,"recent_index":tracker.recent.index,"recent_sum":tracker.recent.sum})
}

fn main() -> Result<(), Box<dyn StdError>> {
    let mut args = std::env::args_os().skip(1);
    let request = args.next().ok_or("expected request.jsonl")?;
    let output = args.next().ok_or("expected output.jsonl")?;
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    let mut output = BufWriter::new(File::create(output)?);
    let mut state = None;
    for line in BufReader::new(File::open(request)?).lines() {
        let frame: Frame = serde_json::from_str(&line?)?;
        if let Some(configuration) = frame.configuration {
            state = Some(State::new(
                &configuration
                    .services
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                configuration.options,
            )?);
        }
        let state = state.as_mut().ok_or("missing first configuration")?;
        let error = match state.update(frame.time, &frame.messages) {
            Ok(()) => None,
            Err(Error::Frequency(frequency::Error::ZeroInterval)) => Some("zero_interval"),
            Err(error) => return Err(error.into()),
        };
        let topics: Vec<_> = state.topics().iter().map(|topic| -> Result<Value, Error> {
            let velocity = if topic.service.name == "carState" {
                let openpilot_cereal::log_capnp::event::Which::CarState(car) = topic.event()?.which().map_err(capnp::Error::from)? else {
                    return Err(Error::Configuration("expected carState payload"));
                };
                Some(f64::from(car?.get_v_ego()))
            } else { None };
            topic.data()?;
            Ok(json!({"name":topic.service.name,"seen":topic.seen,"updated":topic.updated,"receive_time":topic.receive_time,
                "receive_frame":topic.receive_frame,"log_mono_time":topic.log_mono_time,"alive":topic.alive,"frequency_ok":topic.frequency_ok,
                "valid":topic.valid,"polled":topic.polled,"tracker":tracker(&topic.tracker),"velocity":velocity}))
        }).collect::<Result<_,_>>()?;
        let checks: Vec<_> = frame
            .checks
            .iter()
            .map(|names| -> Result<Value, Error> {
                let names: Vec<_> = names.iter().map(String::as_str).collect();
                Ok(json!([
                    state.all_alive(&names)?,
                    state.all_frequency_ok(&names)?,
                    state.all_valid(&names)?,
                    state.all_checks(&names)?
                ]))
            })
            .collect::<Result<_, _>>()?;
        serde_json::to_writer(
            &mut output,
            &json!({"frame":state.frame(),"frequency":state.update_frequency,"topics":topics,"checks":checks,"error":error}),
        )?;
        writeln!(output)?;
    }
    output.flush()?;
    Ok(())
}
