use openpilot_usbgpu::{
    amd_metadata::Catalog,
    discovery::Discovery,
    firmware::{Firmware, FirmwareSource, Segment},
    Error,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};
struct Source(PathBuf);
impl FirmwareSource for Source {
    fn load(&mut self, name: &str, _: &str) -> Result<Vec<u8>, Error> {
        Ok(std::fs::read(self.0.join(name))?)
    }
}
fn segment(segment: &Segment) -> Value {
    json!({"kinds":segment.kinds,"size":segment.data().len(),"sha256":format!("{:x}",Sha256::digest(segment.data()))})
}
fn main() {
    let mut source = Source(std::env::args_os().nth(1).unwrap().into());
    let discovery = Discovery {
        versions: BTreeMap::from([
            (1, [12, 0, 0]),
            (3, [7, 0, 0]),
            (15, [14, 0, 2]),
            (16, [14, 0, 2]),
        ]),
        bases: BTreeMap::new(),
        gc_version: [2, 1],
        gc_info: BTreeMap::new(),
    };
    let firmware = Firmware::load(&Catalog::bundled().unwrap(), &discovery, &mut source).unwrap();
    let sos = firmware.sos.iter().map(|(&kind, blob)| {
        (kind.to_string(), json!({"size": blob.data().len(), "sha256": format!("{:x}", Sha256::digest(blob.data()))}))
    }).collect::<serde_json::Map<_, _>>();
    println!(
        "{}",
        json!({"sos": sos, "descriptors": firmware.descriptors.iter().map(segment).collect::<Vec<_>>(),
                         "smu": firmware.smu.as_ref().map(segment), "ucode_start": firmware.ucode_start})
    );
}
