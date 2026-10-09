use openpilot_usbgpu::{
    queue::{Ring, RingIo},
    Error,
};
use serde_json::{json, Value};
use std::io::{self, BufRead};
struct Trace {
    events: Vec<Value>,
    size: usize,
}
impl RingIo for Trace {
    fn write_word(&mut self, offset: usize, value: u32) -> Result<(), Error> {
        if offset + 4 > self.size {
            return Err(Error::Contract("fixture write outside ring"));
        }
        self.events.push(json!(["word", offset, value]));
        Ok(())
    }
    fn write_bytes(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        if offset + bytes.len() > self.size {
            return Err(Error::Contract("fixture write outside ring"));
        }
        self.events.push(json!(["bytes", offset, bytes]));
        Ok(())
    }
    fn write_pointer(&mut self, value: u64) -> Result<(), Error> {
        self.events.push(json!(["pointer", value]));
        Ok(())
    }
    fn memory_barrier(&mut self) -> Result<(), Error> {
        self.events.push(json!(["barrier"]));
        Ok(())
    }
    fn doorbell(&mut self, value: u64) -> Result<(), Error> {
        self.events.push(json!(["doorbell", value]));
        Ok(())
    }
}
fn main() {
    for line in io::stdin().lock().lines() {
        let v: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut ring = Ring {
            virtual_address: 0x210001000000,
            bytes: v["size"].as_u64().unwrap() as usize,
            put: v["put"].as_u64().unwrap(),
        };
        let words = v["words"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect::<Vec<_>>();
        let mut io = Trace {
            events: vec![],
            size: ring.bytes,
        };
        let bound = v["bound"].as_bool().unwrap();
        let result = match v["kind"].as_str().unwrap() {
            "compute" => ring.submit_compute(&mut io, &words, v["xccs"] == 2, bound),
            "copy" => ring.submit_copy(
                &mut io,
                &words,
                &v["sizes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as usize)
                    .collect::<Vec<_>>(),
                bound,
            ),
            "aql" => ring.submit_aql(
                &mut io,
                &words
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
            _ => panic!("unknown queue kind"),
        };
        println!(
            "{}",
            json!({"events":io.events,"put":ring.put,"failed":result.is_err()})
        );
    }
}
