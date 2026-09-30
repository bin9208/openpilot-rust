use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_torqued::{
    estimator::Estimator, numerics::Numerics, random::RandomState, wire, Error,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, BufRead, Cursor},
    path::Path,
};
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Command {
    New {
        car: Vec<u8>,
        previous: Option<Vec<u8>>,
        saved: Option<Vec<u8>>,
        seed: u32,
        decimated: bool,
        track_all: bool,
    },
    Add {
        points: Vec<[String; 2]>,
    },
    Event {
        bytes: Vec<u8>,
    },
    Message {
        valid: bool,
        points: bool,
    },
    Sample {
        seed: u32,
        populations: Vec<usize>,
        count: usize,
    },
}
fn execute(
    command: Command,
    estimator: &mut Option<Estimator>,
    fit: &mut Numerics,
) -> Result<serde_json::Value, Error> {
    match command {
        Command::Sample {
            seed,
            populations,
            count,
        } => {
            let mut random = RandomState::seeded(seed);
            let samples: Result<Vec<_>, _> = populations
                .into_iter()
                .map(|n| random.sample_indices(n, count))
                .collect();
            return Ok(json!({"samples": samples?}));
        }
        Command::New {
            car,
            previous,
            saved,
            seed,
            decimated,
            track_all,
        } => {
            let mut next = Estimator::new(
                wire::car(&car)?,
                (decimated, track_all),
                RandomState::seeded(seed),
            );
            let removed =
                wire::restore(&mut next, previous.as_deref(), saved.as_deref())?.is_some();
            *estimator = Some(next);
            return Ok(json!({"removed": removed}));
        }
        Command::Add { points } => {
            let current = estimator
                .as_mut()
                .ok_or(Error::Contract("estimator missing"))?;
            for [x, y] in points {
                current.buckets.add(
                    x.parse().map_err(|_| Error::Contract("x invalid"))?,
                    y.parse().map_err(|_| Error::Contract("y invalid"))?,
                );
            }
        }
        Command::Event { bytes } => {
            let reader = serialize::read_message(Cursor::new(bytes), ReaderOptions::new())?;
            let current = estimator
                .as_mut()
                .ok_or(Error::Contract("estimator missing"))?;
            let point = current.handle(wire::input(reader.get_root::<event::Reader<'_>>()?)?)?;
            return Ok(
                json!({"point":point.map(|p|p.map(|v|v.to_string())), "counts":current.buckets.counts(), "all":current.all_points.len()}),
            );
        }
        Command::Message { valid, points } => {
            let current = estimator
                .as_mut()
                .ok_or(Error::Contract("estimator missing"))?;
            let packet = current.message(fit, points)?;
            return Ok(
                json!({"packet":wire::encode(&packet,0,valid)?, "raw":packet.raw.map(|v|v.to_string()), "filtered":packet.filtered.map(|v|v.to_string()), "counts":current.buckets.counts(), "decay":current.decay.to_string()}),
            );
        }
    }
    Ok(
        json!({"counts":estimator.as_ref().ok_or(Error::Contract("estimator missing"))?.buckets.counts()}),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args().nth(1).ok_or("numerics path required")?;
    let mut fit = Numerics::load(Path::new(&directory))?;
    let mut estimator = None;
    for line in io::stdin().lock().lines() {
        let command = serde_json::from_str(&line?)?;
        let output = match execute(command, &mut estimator, &mut fit) {
            Ok(value) => value,
            Err(error) => json!({"error":error.to_string()}),
        };
        println!("{output}");
    }
    Ok(())
}
