use openpilot_usbgpu::{allocator::Tlsf, Error};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{self, BufRead},
};
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut allocator = Tlsf::new(
            input["size"].as_u64().unwrap(),
            input["base"].as_u64().unwrap(),
        )
        .unwrap();
        let mut allocated = HashMap::new();
        let mut results = Vec::new();
        for op in input["operations"].as_array().unwrap() {
            let id = op["id"].as_u64().unwrap();
            if op["kind"] == "alloc" {
                let value = match allocator
                    .alloc(op["size"].as_u64().unwrap(), op["align"].as_u64().unwrap())
                {
                    Ok(address) => {
                        allocated.insert(id, address);
                        json!(address)
                    }
                    Err(Error::Allocation(_)) => Value::Null,
                    Err(error) => panic!("{error}"),
                };
                results.push(value);
            } else if let Some(address) = allocated.remove(&id) {
                allocator.free(address).unwrap();
                results.push(json!(true));
            } else {
                results.push(json!(false));
            }
        }
        println!("{}", json!(results));
    }
}
