use openpilot_pandad::firmware::{
    native_spi::Pool,
    spi::{Error, Io, Mode},
};
use serde_json::{json, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Value = serde_json::from_str(&std::env::var("PANDA_SPI_CASE")?)?;
    let library = std::env::var("PANDA_SPI_LIBRARY")?;
    if !std::fs::read_to_string("/proc/self/maps")?.contains(&library) {
        return Err("owned SPI fixture is not mapped; refusing device access".into());
    }
    let pool = Pool::system();
    let mut results = Vec::new();
    for operation in input["operations"]
        .as_array()
        .ok_or("operations required")?
    {
        let speed = operation["speed"].as_u64().unwrap_or(50_000_000) as u32;
        let result = (|| -> Result<Value, Error> {
            let Some(mut io) = pool.open(speed, |_, _| Ok(()))? else {
                return Ok(json!({"missing":true}));
            };
            if operation["kind"] == "open" {
                return Ok(Value::Null);
            }
            io.lock()?;
            let value = {
                let data: Vec<u8> = operation["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|v| v.as_u64().expect("byte") as u8)
                    .collect();
                let length = operation["length"].as_u64().unwrap_or(1) as usize;
                match operation["kind"].as_str() {
                    Some("xfer") => io.transfer(Mode::Xfer, &data).map(|v| json!(v)),
                    Some("xfer2") => io.transfer(Mode::Xfer2, &data).map(|v| json!(v)),
                    Some("read") => io.read(length).map(|v| json!(v)),
                    Some("write") => io.write(&data).map(|()| Value::Null),
                    Some("kernel") => io
                        .kernel(
                            operation["endpoint"].as_u64().unwrap_or(0) as u8,
                            &data,
                            length,
                            operation["disconnect"].as_bool().unwrap_or(false),
                        )
                        .map(|v| json!(v)),
                    _ => Err(Error::Invalid("unknown operation")),
                }
            };
            io.unlock()?;
            value
        })();
        results.push(match result {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(error) => json!({"ok":false,"error":match error {
                Error::Invalid(_) => "invalid",
                Error::Io(_) => "io",
                _ => "protocol",
            }}),
        });
    }
    drop(pool);
    println!("{}", json!(results));
    Ok(())
}
