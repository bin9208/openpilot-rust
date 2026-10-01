use openpilot_athena::{ipc, Error};
use std::io::{self, BufRead, Write};
fn main() -> Result<(), Error> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("decode") => {
            for line in io::stdin().lock().lines() {
                let bytes: Vec<u8> = serde_json::from_str(&line?)?;
                match ipc::decode(&bytes) {
                    Ok(value) => println!("{{\"result\":{}}}", value.to_json()?),
                    Err(error) => println!("{}", serde_json::json!({"error":error.to_string()})),
                }
            }
        }
        Some("publish") => {
            let name = args.get(1).ok_or(Error::Contract("service"))?;
            let service =
                openpilot_messaging::services::lookup(name).ok_or(Error::Contract("service"))?;
            let mut publisher = openpilot_msgq::Publisher::for_runtime(name, service.queue_size)?;
            println!("READY");
            io::stdout().flush()?;
            for line in io::stdin().lock().lines() {
                let bytes: Vec<u8> = serde_json::from_str(&line?)?;
                publisher.send(&bytes)?;
                println!("OK");
                io::stdout().flush()?;
            }
        }
        _ => return Err(Error::Contract("expected decode or publish")),
    }
    Ok(())
}
