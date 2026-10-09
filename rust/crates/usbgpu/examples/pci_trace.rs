use openpilot_usbgpu::{
    pci::{setup_bars, Config},
    Error,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{self, BufRead},
};
struct Fixture {
    values: HashMap<u16, u32>,
    masks: HashMap<u16, u32>,
    trace: Vec<Value>,
}
impl Config for Fixture {
    fn read_config(&mut self, bus: u8, offset: u16, size: u8) -> Result<u32, Error> {
        self.trace
            .push(json!({"bus":bus,"offset":offset,"size":size,"value":null}));
        Ok(if self.values.get(&offset) == Some(&u32::MAX) {
            self.masks.get(&offset).copied().unwrap_or(u32::MAX)
        } else {
            self.values.get(&offset).copied().unwrap_or(0)
        })
    }
    fn write_config(&mut self, bus: u8, offset: u16, size: u8, value: u32) -> Result<(), Error> {
        self.trace
            .push(json!({"bus":bus,"offset":offset,"size":size,"value":value}));
        if bus == 4 {
            self.values.insert(offset, value);
        }
        Ok(())
    }
}
fn values(input: &Value) -> HashMap<u16, u32> {
    input
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.parse().unwrap(), value.as_u64().unwrap() as u32))
        .collect()
}
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut fixture = Fixture {
            values: values(&input["values"]),
            masks: values(&input["masks"]),
            trace: Vec::new(),
        };
        let bars = setup_bars(&mut fixture, 4, 0x10000000, 32 << 30).unwrap();
        let bars = bars
            .into_iter()
            .map(|(index, bar)| (index.to_string(), json!([bar.address, bar.size])))
            .collect::<serde_json::Map<_, _>>();
        println!("{}", json!({"bars":bars,"trace":fixture.trace}));
    }
}
