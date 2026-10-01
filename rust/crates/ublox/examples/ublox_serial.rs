use openpilot_ublox::{serial::Serial, Error};
use serde_json::json;
use std::io::Write;
fn main() -> Result<(), Error> {
    let path = std::env::args()
        .nth(1)
        .ok_or(Error::Malformed("serial path"))?;
    let mut port = Serial::open(std::path::Path::new(&path))?;
    println!("{{\"ready\":true}}");
    std::io::stdout().flush()?;
    for input in std::io::stdin().lines() {
        let action = input?;
        match action.as_str() {
            "read" => println!("{}", json!({"data":port.receive()?})),
            "baud" => {
                port.baud(460800)?;
                println!("{{\"baud\":460800}}");
            }
            "send" => {
                port.send(b"\x00\xb5\x62\xff\r\n")?;
                println!("{{\"sent\":true}}");
            }
            _ => return Err(Error::Malformed("serial action")),
        }
        std::io::stdout().flush()?;
    }
    Ok(())
}
