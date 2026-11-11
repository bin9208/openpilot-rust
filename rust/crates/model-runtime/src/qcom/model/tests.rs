use super::super::{device::Memory, QcomGraph};
use super::*;
use std::{cell::Cell, io, rc::Rc};

struct HostMemory(Vec<u8>);
impl Memory for HostMemory {
    fn address(&self) -> u64 {
        0x1000000
    }
    fn bytes(&self) -> &[u8] {
        &self.0
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

struct HostDriver(Rc<Cell<u32>>, Rc<Cell<usize>>);
impl Driver for HostDriver {
    type Memory = HostMemory;
    fn allocate(&mut self, size: usize) -> io::Result<HostMemory> {
        self.1.set(self.1.get() + 1);
        Ok(HostMemory(vec![0xa5; size]))
    }
    fn submit(&mut self, _address: u64, _size: usize) -> io::Result<u32> {
        self.0.set(self.0.get() + 1);
        Ok(self.0.get())
    }
    fn wait(&mut self, timestamp: u32) -> io::Result<()> {
        assert_eq!(timestamp, self.0.get());
        Ok(())
    }
    fn close(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn model_preserves_aliases_copies_and_state_between_runs() {
    let graph = QcomGraph::parse(br#"{"version":1,"backend":"qcom-cl","arch":"a630",
        "weights_sha256":"0000000000000000000000000000000000000000000000000000000000000000",
        "allocations":[{"bytes":16,"weight_offset":0},{"bytes":8,"weight_offset":null}],
        "views":[{"allocation":0,"offset":0,"bytes":8},{"allocation":0,"offset":4,"bytes":8},{"allocation":1,"offset":0,"bytes":8}],
        "inputs":[{"name":"state","view":0}],"outputs":[{"name":"state","view":1},{"name":"copy","view":2}],
        "kernels":[],"calls":[{"op":"copy","source":0,"destination":1},{"op":"copy","source":1,"destination":2}]}"#).unwrap();
    let bundle = QcomBundle {
        graph,
        programs: vec![],
        weights: (1..=16).collect(),
    };
    let submissions = Rc::new(Cell::new(0));
    let mut model = Model::new(
        bundle,
        HostDriver(submissions.clone(), Rc::new(Cell::new(0))),
    )
    .unwrap();
    assert_eq!(model.read_output("copy").unwrap(), &[0; 8]);
    model.run().unwrap();
    assert_eq!(
        model.read_output("state").unwrap(),
        &[1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(
        model.read_output("copy").unwrap(),
        &[1, 2, 3, 4, 5, 6, 7, 8]
    );
    model.run().unwrap();
    assert_eq!(
        model.read_output("state").unwrap(),
        &[1, 2, 3, 4, 1, 2, 3, 4]
    );
    model.write_input("state", &[42; 8]).unwrap();
    model.run().unwrap();
    assert_eq!(model.read_output("copy").unwrap(), &[42; 8]);
    assert!(model.write_input("state", &[0]).is_err());
    assert!(model.write_input("missing", &[0; 8]).is_err());
    assert!(model.read_output("missing").is_err());
    model.reset_mutable().unwrap();
    assert_eq!(model.read_output("copy").unwrap(), &[0; 8]);
    assert_eq!(
        model.read_output("state").unwrap(),
        &[5, 6, 7, 8, 9, 10, 11, 12]
    );
    assert_eq!(submissions.get(), 0);
}
