use openpilot_usbgpu::{amd_metadata::Catalog, discovery::Discovery};
use serde_json::{json, Value};
use std::io::{self, BufRead};
fn main() {
    let catalog = Catalog::bundled().unwrap();
    for line in io::stdin().lock().lines() {
        let value: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let bytes = value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        match Discovery::parse(&catalog, &bytes) {
            Ok(discovery) => {
                let regs = discovery.registers(&catalog).unwrap();
                let regs = regs
                    .into_iter()
                    .map(|(name, register)| {
                        (
                            name,
                            json!({"addresses":register.addresses,"fields":register.fields}),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>();
                println!("{}", json!({"discovery":discovery,"registers":regs}));
            }
            Err(error) => println!("{}", json!({"error":error.to_string()})),
        }
    }
}
