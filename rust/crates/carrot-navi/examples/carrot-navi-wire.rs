use openpilot_carrot_navi::{json::Value, native::wire, Error};
use std::{
    fmt::Write,
    io::{self, Read},
};

fn run() -> Result<(), Error> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| Error::typed("OSError", error.to_string()))?;
    let Value::Array(snapshots) = Value::parse(&input)? else {
        return Err(Error::value("wire fixtures must be an array"));
    };
    let mut rows = Vec::with_capacity(snapshots.len());
    for snapshot in snapshots {
        let result = match wire::state_at(&snapshot, 777, || 999) {
            Ok(bytes) => {
                let mut hex = String::with_capacity(bytes.len() * 2);
                for byte in bytes {
                    write!(hex, "{byte:02x}").map_err(|_| Error::value("hex formatting failed"))?;
                }
                Value::object([("wire_hex", Value::text(&hex)), ("error", Value::Null)])
            }
            Err(error) => Value::object([
                ("wire_hex", Value::Null),
                (
                    "error",
                    Value::object([
                        ("type", Value::text(error.kind)),
                        ("message", error.message_value()),
                    ]),
                ),
            ]),
        };
        rows.push(result);
    }
    println!("{}", Value::Array(rows).encode()?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{}: {error}", error.kind);
        std::process::exit(1);
    }
}
