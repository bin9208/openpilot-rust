use openpilot_athena::{
    methods, rpc,
    state::{Config, Shared, Stop},
    Error,
};
use std::{
    io::{self, BufRead},
    sync::Arc,
};
fn main() -> Result<(), Error> {
    let shared = Arc::new(Shared::new(Config::for_runtime()?)?);
    let stop = Stop::default();
    let mut logger = shared.factory.logger();
    for line in io::stdin().lock().lines() {
        let line: serde_json::Value = serde_json::from_str(&line?)?;
        let data = line["data"].as_str().ok_or(Error::Contract("data"))?;
        let output = rpc::route(
            data,
            line["binary"].as_bool().unwrap_or(false),
            |method, args| methods::call(&shared, &stop, &mut logger, method, args),
        );
        match output {
            rpc::Route::Reply(text) => println!("{{\"reply\":{text}}}"),
            rpc::Route::LogResponse(text) => println!("{}", serde_json::json!({"log":text})),
        }
    }
    Ok(())
}
