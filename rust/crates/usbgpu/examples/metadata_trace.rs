use openpilot_usbgpu::{amd_metadata::Catalog, Error};
use serde_json::{json, Value};
use std::io::{self, BufRead};
fn run(catalog: &Catalog, value: &Value) -> Result<Value, Error> {
    if let Some(module) = value["module"].as_str() {
        return Ok(json!(catalog.encode_macro(
            module,
            value["name"].as_str().unwrap(),
            value["value"].as_u64().unwrap() as u32
        )?));
    }
    let record = value["record"].as_str().unwrap();
    let path = [value["field"].as_str().unwrap()];
    let mut bytes = value["bytes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect::<Vec<_>>();
    if let Some(number) = value["write"].as_u64() {
        catalog.write(record, &path, &mut bytes, number)?;
    }
    let number = if value["signed"].as_bool().unwrap_or(false) {
        json!(catalog.read_signed(record, &path, &bytes)?)
    } else {
        json!(catalog.read(record, &path, &bytes)?)
    };
    Ok(json!({"bytes":bytes,"value":number}))
}
fn main() {
    let catalog = Catalog::bundled().unwrap();
    for line in io::stdin().lock().lines() {
        let value: Value = serde_json::from_str(&line.unwrap()).unwrap();
        match run(&catalog, &value) {
            Ok(result) => println!("{result}"),
            Err(error) => println!("{}", json!({"error":error.to_string()})),
        }
    }
}
