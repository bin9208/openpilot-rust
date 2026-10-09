use openpilot_usbgpu::{
    hcq_model::{Allocation, Device, Model, Request},
    hcq_vm::{Function, Host, Memory},
    Error,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Default)]
struct OwnedDevice {
    buffers: BTreeMap<u64, Vec<u8>>,
    fail_poll: Option<Arc<AtomicUsize>>,
}
impl Host for OwnedDevice {
    fn poll(&mut self) -> Result<(), Error> {
        if let Some(count) = &self.fail_poll {
            count.fetch_add(1, Ordering::SeqCst);
            return Err(Error::Contract("owned transport fault"));
        }
        Ok(())
    }
    fn call(&mut self, _: Function, _: &[u64], _: &mut Memory) -> Result<u64, Error> {
        Err(Error::Contract("unexpected USB call"))
    }
}
impl Device for OwnedDevice {
    fn allocate(&mut self, request: Request<'_>) -> Result<Allocation, Error> {
        let key = self.buffers.len() as u64;
        self.buffers.insert(key, vec![0; request.bytes as usize]);
        Ok(Allocation {
            key,
            device: 0x1000 + key * 4096,
            host: 0x10000 + key * 4096,
            bytes: request.bytes,
        })
    }
    fn write(&mut self, buffer: Allocation, offset: u64, data: &[u8]) -> Result<(), Error> {
        self.buffers.get_mut(&buffer.key).unwrap()[offset as usize..offset as usize + data.len()]
            .copy_from_slice(data);
        Ok(())
    }
    fn read(&mut self, buffer: Allocation, data: &mut [u8]) -> Result<(), Error> {
        data.copy_from_slice(&self.buffers[&buffer.key][..data.len()]);
        Ok(())
    }
    fn synchronize(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

#[test]
fn verified_model_preserves_recurrent_alias_and_rejects_changed_artifact() {
    let temp = tempfile::tempdir().unwrap();
    let artifact = temp.path().join("model.pkl");
    std::fs::write(&artifact, b"abcd").unwrap();
    let hash = format!("{:x}", Sha256::digest(b"abcd"));
    let bundle = json!({"version":1,"model_sha256":hash,"model_bytes":4,"kernel_count":1,
        "buffers":[{"kind":"allocation","bytes":4,"host":false,"cpu_access":true,"uncached":false,
            "initial":{"offset":0,"bytes":4,"sha256":hash}},
            {"kind":"placeholder","tag":"inputs","bytes":16,"elements":2,"host":false,"cpu_access":true,"uncached":false,"device":"CPU"}],
        "patches":[],"arguments":[{"buffer":1,"offset":0}],"parameters":[],
        "bindings":[{"name":"state","shape":[4],"bytes":4,"dtype":"uint8","output":false,"alias":null},
            {"name":"next_state","shape":[4],"bytes":4,"dtype":"uint8","output":true,"alias":0}],
        "input_table":{"argument":0,"entries":[{"slot":0,"offset":0},{"slot":1,"offset":0}]},
        "dispatcher":[{"op":{"kind":"noop"},"src":[]}]});
    let descriptor = serde_json::to_vec(&bundle).unwrap();
    let mut model = Model::load(&descriptor, &artifact, OwnedDevice::default()).unwrap();
    model.write_input("state", b"WXYZ").unwrap();
    model.run().unwrap();
    let mut output = [0; 4];
    model.read_output("next_state", &mut output).unwrap();
    assert_eq!(&output, b"WXYZ");
    assert!(model.write_input("state", b"wrong size").is_err());
    let polls = Arc::new(AtomicUsize::new(0));
    let mut failed = Model::load(
        &descriptor,
        &artifact,
        OwnedDevice {
            fail_poll: Some(Arc::clone(&polls)),
            ..OwnedDevice::default()
        },
    )
    .unwrap();
    assert!(failed.run().is_err());
    assert!(failed.run().is_err());
    assert!(failed.write_input("state", b"abcd").is_err());
    assert!(failed.read_output("next_state", &mut output).is_err());
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    std::fs::write(&artifact, b"abce").unwrap();
    assert!(Model::load(&descriptor, &artifact, OwnedDevice::default()).is_err());
}
