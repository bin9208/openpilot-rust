use openpilot_usbgpu::{
    amd_metadata::Catalog,
    packets::{CachePolicy, ComputePackets, CopyPackets, Release},
    Error,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, BufRead},
};
fn number(v: &Value, key: &str) -> u64 {
    v[key].as_u64().unwrap_or(0)
}
fn run(catalog: &Catalog, v: &Value) -> Result<Value, Error> {
    let registers = BTreeMap::new();
    let gfx = number(v, "gfx") as u8;
    if v["kind"] == "compute" {
        let mut q = ComputePackets::new(catalog, &registers, gfx, number(v, "xccs") as u8);
        for op in v["ops"].as_array().unwrap() {
            let addr = number(op, "address");
            let value = number(op, "value");
            match op["op"].as_str().unwrap() {
                "wait" => q.wait(
                    addr,
                    value as u32,
                    number(op, "mask") as u32,
                    number(op, "operation") as u32,
                )?,
                "acquire" => q.acquire(
                    addr,
                    number(op, "size"),
                    CachePolicy {
                        gli: number(op, "gli") as u32,
                        glm: number(op, "glm") as u32,
                        glk: number(op, "glk") as u32,
                        glv: number(op, "glv") as u32,
                        gl1: number(op, "gl1") as u32,
                        gl2: number(op, "gl2") as u32,
                    },
                )?,
                "release" => q.release(Release {
                    address: addr,
                    value,
                    data_select: number(op, "data") as u32,
                    interrupt_select: number(op, "interrupt") as u32,
                    context: number(op, "context") as u32,
                    flush: op["flush"].as_bool().unwrap(),
                })?,
                "signal" => q.signal(addr, value as u32)?,
                "timestamp" => q.timestamp(addr)?,
                "write" => q.write(addr, value, op["wide"].as_bool().unwrap())?,
                _ => return Err(Error::Contract("unknown packet fixture operation")),
            }
        }
        Ok(json!({"words":q.words,"indirect":q.indirect(0x210001000000)?}))
    } else {
        let mut q = CopyPackets::new(catalog, number(v, "sdma") as u8, gfx)?;
        q.max_copy_size = number(v, "max_copy");
        for op in v["ops"].as_array().unwrap() {
            let addr = number(op, "address");
            let value = number(op, "value");
            match op["op"].as_str().unwrap() {
                "copy" => q.copy(addr, number(op, "source"), number(op, "size"))?,
                "signal" => q.signal(addr, value as u32, op["owned"].as_bool().unwrap())?,
                "wait" => q.wait(addr, value as u32)?,
                "timestamp" => q.timestamp(addr)?,
                "write" => q.write(addr, value, op["wide"].as_bool().unwrap())?,
                _ => return Err(Error::Contract("unknown copy fixture operation")),
            }
        }
        let (padded, indirect) = q.indirect(0x210002000000)?;
        Ok(json!({"words":q.words,"sizes":q.command_sizes,"padded":padded,"indirect":indirect}))
    }
}
fn main() {
    let catalog = Catalog::bundled().unwrap();
    for line in io::stdin().lock().lines() {
        let value: Value = serde_json::from_str(&line.unwrap()).unwrap();
        match run(&catalog, &value) {
            Ok(value) => println!("{value}"),
            Err(error) => println!("{}", json!({"error":error.to_string()})),
        }
    }
}
