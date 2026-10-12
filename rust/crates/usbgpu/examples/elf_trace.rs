use openpilot_usbgpu::{elf::Image, Error};
use serde_json::{json, Value};
use std::io::{self, BufRead};
fn run(path: &str) -> Result<Value, Error> {
    let mut image = Image::parse(&std::fs::read(path)?)?;
    image.relocate_amd()?;
    Ok(json!({"bytes":image.bytes,"sections":image.sections,"relocations":image.relocations}))
}
fn main() {
    for line in io::stdin().lock().lines() {
        match run(&line.unwrap()) {
            Ok(value) => println!("{value}"),
            Err(error) => println!("{}", json!({"error":error.to_string()})),
        }
    }
}
