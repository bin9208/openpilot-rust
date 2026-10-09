use openpilot_usbgpu::{
    hcq_vm::{Function, Host, Memory, Program},
    Error,
};
use serde_json::json;

struct Transfers;
impl Host for Transfers {
    fn poll(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn call(
        &mut self,
        function: Function,
        arguments: &[u64],
        memory: &mut Memory,
    ) -> Result<u64, Error> {
        assert_eq!(function, Function::Bulk);
        assert_eq!(memory.read(arguments[2], arguments[3] as usize)?, b"abc");
        memory.write(arguments[4], &3_i32.to_le_bytes())?;
        Ok(0)
    }
}

#[test]
fn dispatcher_owns_byte_ranges_and_transport_output() {
    let mut memory = Memory::default();
    let data = memory.allocate(b"abc".to_vec()).unwrap();
    let transferred = memory.allocate(vec![0; 4]).unwrap();
    let function = memory.allocate(2_u64.to_le_bytes().to_vec()).unwrap();
    let nodes = json!([
      {"op":{"kind":"argument","slot":0},"src":[],"bits":8,"pointer":true},
      {"op":{"kind":"argument","slot":1},"src":[],"bits":32,"pointer":true},
      {"op":{"kind":"argument","slot":2},"src":[],"bits":64,"pointer":true},
      {"op":{"kind":"load"},"src":[2],"bits":64},
      {"op":{"kind":"function","function":"bulk"},"src":[3]},
      {"op":{"kind":"constant","value":0},"src":[],"bits":64},
      {"op":{"kind":"constant","value":2},"src":[],"bits":32},
      {"op":{"kind":"constant","value":3},"src":[],"bits":32},
      {"op":{"kind":"constant","value":1000},"src":[],"bits":32},
      {"op":{"kind":"call"},"src":[4,5,6,0,7,1,8],"bits":32,"signed":true}
    ]);
    let program = Program::parse(&serde_json::to_vec(&nodes).unwrap()).unwrap();
    let mut execution = program
        .bind(&mut memory, &[data, transferred, function])
        .unwrap();
    execution.run(&mut memory, &mut Transfers).unwrap();
    assert_eq!(memory.read(transferred, 4).unwrap(), 3_i32.to_le_bytes());
    assert!(memory.read(data + 2, 2).is_err());
}
