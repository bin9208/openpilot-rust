//! Source manager-entry latch actions; injected sleep never delays the test.
use openpilot_signal_observer::lifecycle;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    io::{self, Read},
    sync::atomic::AtomicBool,
};
#[derive(Deserialize)]
struct Case {
    failed: bool,
    waits: usize,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Case> = serde_json::from_str(&text)?;
    let mut output = Vec::new();
    for case in cases {
        let actions = RefCell::new(vec![json!(["run", case.failed])]);
        let mut n = 0;
        lifecycle::finish(
            if case.failed { Err(()) } else { Ok(()) },
            |()| {
                actions
                    .borrow_mut()
                    .push(json!(["exception", lifecycle::FAILURE_MESSAGE]));
                Ok::<_, ()>(())
            },
            || {
                let enabled = n < case.waits;
                n += 1;
                actions.borrow_mut().push(json!(["requested", enabled]));
                enabled
            },
            &AtomicBool::new(false),
            |duration| {
                actions
                    .borrow_mut()
                    .push(json!(["sleep", duration.as_secs()]))
            },
        )
        .map_err(|()| "injected report failure")?;
        output.push(actions.into_inner());
    }
    println!("{}", serde_json::to_string::<Vec<Vec<Value>>>(&output)?);
    Ok(())
}
