use openpilot_usbgpu::page_table::{fragment, AddressSpace, PteOptions, PtePolicy};
use serde_json::Value;
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let v: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let result = if v["kind"] == "flags" {
            let policy = PtePolicy {
                gfx_major: v["gfx"].as_u64().unwrap() as u8,
                uncached_type: 3,
                address_mask: (1 << 44) - 1,
                physical_base: 0,
            };
            let flags = policy.flags(
                v["level"].as_u64().unwrap() as u8,
                PteOptions {
                    table: v["table"].as_bool().unwrap(),
                    uncached: v["uncached"].as_bool().unwrap(),
                    space: if v["system"].as_bool().unwrap() {
                        AddressSpace::System
                    } else {
                        AddressSpace::Physical
                    },
                    snooped: v["snooped"].as_bool().unwrap(),
                    fragment: v["fragment"].as_u64().unwrap() as u8,
                    valid: v["valid"].as_bool().unwrap(),
                },
            );
            serde_json::json!({"flags":flags,"page":policy.is_page(v["level"].as_u64().unwrap() as u8,flags)})
        } else {
            serde_json::json!(fragment(
                v["address"].as_u64().unwrap(),
                v["size"].as_u64().unwrap(),
                v["must_cover"].as_bool().unwrap()
            )
            .unwrap())
        };
        println!("{result}");
    }
}
