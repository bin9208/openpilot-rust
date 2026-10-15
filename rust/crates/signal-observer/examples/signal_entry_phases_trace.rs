use openpilot_plannerd::signal::transport::Mode;
use openpilot_signal_observer::{
    backend::Request,
    entry::{self, Route},
    runtime::Error,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    cell::RefCell,
    io::{self, Read},
};
#[derive(Deserialize)]
struct Case {
    initial: i64,
    enabled: bool,
    flags: Vec<String>,
    backend: String,
}
fn main() -> Result<(), Error> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let mut output = vec![];
    for case in cases {
        let trace = RefCell::new(vec![]);
        let mut flags = case.flags.iter();
        let result = entry::select(
            &mut || {
                trace.borrow_mut().push("mode");
                Mode::from_parameter(case.initial)
            },
            &mut || {
                trace.borrow_mut().push("neural");
                match flags.next().unwrap().as_str() {
                    "1" => Ok(true),
                    "0" => Ok(false),
                    _ => Err("fixture decode failure".into()),
                }
            },
            &mut || {
                trace.borrow_mut().push("requested");
                Ok(case.enabled)
            },
            &mut || {
                trace.borrow_mut().push("backend");
                Ok(Request::parse(Some(&case.backend))?)
            },
        );
        let route = match result {
            Ok(Route::Classical) => "classical",
            Ok(Route::Disabled) => "disabled",
            Ok(Route::Neural(Request::Cpu)) => "cpu",
            Ok(Route::Neural(_)) => "automatic",
            Err(_) => "error",
        };
        output.push(json!({"route":route,"trace":trace.into_inner()}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
