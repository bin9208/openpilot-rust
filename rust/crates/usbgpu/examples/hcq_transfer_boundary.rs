use openpilot_usbgpu::{
    hcq_gpu,
    hcq_vm::{Function, Host, Memory, Program},
    native_usb::Usb,
    transport::{Setup, Transport},
    Error,
};
use serde_json::{json, Value};
struct Boundary(Usb);
impl Host for Boundary {
    fn poll(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn call(
        &mut self,
        function: Function,
        args: &[u64],
        memory: &mut Memory,
    ) -> Result<u64, Error> {
        hcq_gpu::transfer(&mut self.0, function, args, memory)
    }
}
fn main() {
    let mode = std::env::var("USB_FIXTURE_MODE").unwrap();
    let bulk = matches!(mode.as_str(), "partial" | "bulk_short" | "bulk_ok");
    let mut boundary = Boundary(Usb::open(0xadd1, 1, 0).unwrap().unwrap());
    boundary.0.setup(Setup::Claim, 0, 0).unwrap();
    let mut memory = Memory::default();
    let data = memory.allocate(vec![0; 12]).unwrap();
    let actual = memory.allocate(vec![0; 4]).unwrap();
    let function = memory
        .allocate((if bulk { 2u64 } else { 1u64 }).to_le_bytes().to_vec())
        .unwrap();
    let mut nodes: Vec<Value> = vec![
        json!({"op":{"kind":"argument","slot":0},"src":[],"bits":8,"pointer":true}),
        json!({"op":{"kind":"argument","slot":1},"src":[],"bits":32,"pointer":true}),
        json!({"op":{"kind":"argument","slot":2},"src":[],"bits":64,"pointer":true}),
        json!({"op":{"kind":"load"},"src":[2],"bits":64}),
        json!({"op":{"kind":"function","function":if bulk{"bulk"}else{"control"}},"src":[3]}),
    ];
    let values = if bulk {
        vec![1, 2, 0, 12, 0, 1000]
    } else {
        vec![1, 64, 240, 0, 0, 0, 12, 5000]
    };
    let mut src = vec![4];
    for (index, value) in values.into_iter().enumerate() {
        if index == if bulk { 2 } else { 5 } {
            src.push(0);
        } else if bulk && index == 4 {
            src.push(1);
        } else {
            src.push(nodes.len());
            nodes.push(json!({"op":{"kind":"constant","value":value},"src":[],"bits":64}));
        }
    }
    nodes.push(json!({"op":{"kind":"call"},"src":src,"bits":32,"signed":true}));
    nodes.push(json!({"op":{"kind":"call"},"src":src,"bits":32,"signed":true}));
    let program = Program::parse(&serde_json::to_vec(&nodes).unwrap()).unwrap();
    let mut execution = program
        .bind(&mut memory, &[data, actual, function])
        .unwrap();
    let result = execution.run(&mut memory, &mut boundary);
    assert_eq!(result.is_ok(), matches!(mode.as_str(), "ok" | "bulk_ok"));
    println!(
        "{}",
        json!({"mode":mode,"error":result.err().map(|e|e.to_string()),"transferred":memory.read(actual,4).unwrap()})
    );
    drop(boundary);
}
