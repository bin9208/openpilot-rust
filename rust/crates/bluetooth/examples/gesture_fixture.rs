use openpilot_bluetooth::{Decoder, Event, Mapping, Profile, Seconds, Token};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::{self, BufRead};

#[derive(Deserialize)]
struct Case {
    profile: Profile,
    mapping: Mapping,
    learning: bool,
    operations: Vec<Operation>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Operation {
    Feed { event: Event },
    Flush { at: Seconds },
    Cancel,
}

#[derive(Serialize)]
struct Step {
    tokens: Vec<Token>,
    active: BTreeSet<Token>,
    repeated: BTreeSet<Token>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let case: Case = serde_json::from_str(&line?)?;
        let mut decoder = Decoder::new(case.profile, case.mapping, case.learning);
        let mut result = Vec::with_capacity(case.operations.len());
        for operation in case.operations {
            let tokens = match operation {
                Operation::Feed { event } => decoder.feed(event),
                Operation::Flush { at } => decoder.flush(at),
                Operation::Cancel => {
                    decoder.cancel_holds();
                    Vec::new()
                }
            };
            result.push(Step {
                tokens,
                active: decoder.active_longs(),
                repeated: decoder.repeated().clone(),
            });
        }
        println!("{}", serde_json::to_string(&result)?);
    }
    Ok(())
}
