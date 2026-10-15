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

struct NoTransfers;
impl Host for NoTransfers {
    fn poll(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn call(&mut self, _: Function, _: &[u64], _: &mut Memory) -> Result<u64, Error> {
        panic!("control flow must not invoke transport")
    }
}

#[test]
fn conditional_blocks_skip_writes_and_signed_division_truncates_toward_zero() {
    let nodes = json!([
        {"op":{"kind":"argument","slot":0},"src":[],"bits":64,"pointer":true},
        {"op":{"kind":"constant","value":0},"src":[],"bits":1},
        {"op":{"kind":"if"},"src":[1]},
        {"op":{"kind":"constant","value":99},"src":[],"bits":64},
        {"op":{"kind":"store"},"src":[0,3],"bits":64},
        {"op":{"kind":"endif"},"src":[2]},
        {"op":{"kind":"constant","value":7},"src":[],"bits":64,"signed":true},
        {"op":{"kind":"neg"},"src":[6],"bits":64,"signed":true},
        {"op":{"kind":"constant","value":3},"src":[],"bits":64,"signed":true},
        {"op":{"kind":"cdiv"},"src":[7,8],"bits":64,"signed":true},
        {"op":{"kind":"store"},"src":[0,9],"bits":64}
    ]);
    let mut memory = Memory::default();
    let output = memory.allocate(vec![0; 8]).unwrap();
    let mut execution = Program::parse(&serde_json::to_vec(&nodes).unwrap())
        .unwrap()
        .bind(&mut memory, &[output])
        .unwrap();
    execution.run(&mut memory, &mut NoTransfers).unwrap();
    assert_eq!(memory.read(output, 8).unwrap(), (-2_i64).to_le_bytes());
}

#[test]
fn conditional_blocks_require_balanced_lexical_nesting() {
    for nodes in [
        json!([{"op":{"kind":"constant","value":1},"src":[],"bits":1},{"op":{"kind":"if"},"src":[0]}]),
        json!([{"op":{"kind":"constant","value":1},"src":[],"bits":1},{"op":{"kind":"endif"},"src":[0]}]),
        json!([
          {"op":{"kind":"constant","value":1},"src":[],"bits":1},
          {"op":{"kind":"if"},"src":[0]},
          {"op":{"kind":"range"},"src":[0],"bits":64},
          {"op":{"kind":"endif"},"src":[1]},
          {"op":{"kind":"end"},"src":[0,2]}
        ]),
    ] {
        assert!(Program::parse(&serde_json::to_vec(&nodes).unwrap()).is_err());
    }
}

#[test]
fn fence_clock_writes_monotonic_timespec_to_owned_memory() {
    let nodes = json!([
        {"op":{"kind":"local","bytes":16},"src":[],"bits":64,"pointer":true},
        {"op":{"kind":"constant","value":1},"src":[],"bits":32},
        {"op":{"kind":"clock_gettime"},"src":[1,0],"bits":32,"signed":true},
        {"op":{"kind":"argument","slot":0},"src":[],"bits":64,"pointer":true},
        {"op":{"kind":"load"},"src":[0],"bits":64},
        {"op":{"kind":"store"},"src":[3,4],"bits":64}
    ]);
    let mut memory = Memory::default();
    let output = memory.allocate(vec![0; 8]).unwrap();
    let mut execution = Program::parse(&serde_json::to_vec(&nodes).unwrap())
        .unwrap()
        .bind(&mut memory, &[output])
        .unwrap();
    execution.run(&mut memory, &mut NoTransfers).unwrap();
    let seconds = u64::from_le_bytes(memory.read(output, 8).unwrap().try_into().unwrap());
    assert!(seconds > 0);
    assert!(seconds < 100 * 365 * 86400);
}

#[test]
fn persistent_dispatcher_exact_artifact_lowering_parses_when_available() {
    let Some(path) = std::env::var_os("OPENPILOT_MDM_DISPATCHER_FIXTURE") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    Program::parse(&bytes).unwrap();
}
