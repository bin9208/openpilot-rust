use super::{Device, Driver, Memory};
use std::{cell::RefCell, io, rc::Rc};

struct FakeMemory {
    bytes: Vec<u8>,
    events: Rc<RefCell<Vec<&'static str>>>,
}

impl Memory for FakeMemory {
    fn address(&self) -> u64 {
        0x123400000
    }
    fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}

impl Drop for FakeMemory {
    fn drop(&mut self) {
        self.events.borrow_mut().push("unmap");
    }
}

struct FakeDriver {
    events: Rc<RefCell<Vec<&'static str>>>,
    fail: Option<&'static str>,
}

impl Driver for FakeDriver {
    type Memory = FakeMemory;
    fn allocate(&mut self, size: usize) -> io::Result<FakeMemory> {
        self.events.borrow_mut().push("allocate");
        if self.fail == Some("allocate") {
            return Err(io::Error::other("allocate"));
        }
        Ok(FakeMemory {
            bytes: vec![0; size],
            events: self.events.clone(),
        })
    }
    fn submit(&mut self, _address: u64, _size: usize) -> io::Result<u32> {
        self.events.borrow_mut().push("submit");
        if self.fail == Some("submit") {
            return Err(io::Error::other("submit"));
        }
        Ok(0xffff_ffff)
    }
    fn wait(&mut self, timestamp: u32) -> io::Result<()> {
        assert_eq!(timestamp, 0xffff_ffff);
        self.events.borrow_mut().push("wait");
        if matches!(self.fail, Some("wait" | "wait_and_close")) {
            return Err(io::Error::other("wait"));
        }
        Ok(())
    }
    fn close(&mut self) -> io::Result<()> {
        self.events.borrow_mut().push("close");
        if self.fail == Some("wait_and_close") {
            return Err(io::Error::other("close"));
        }
        Ok(())
    }
}

fn device(fail: Option<&'static str>) -> (Device<FakeDriver>, Rc<RefCell<Vec<&'static str>>>) {
    let events = Rc::new(RefCell::new(Vec::new()));
    (
        Device::new(FakeDriver {
            events: events.clone(),
            fail,
        }),
        events,
    )
}

#[test]
fn device_owns_memory_until_context_is_closed() {
    let (mut device, events) = device(None);
    let buffer = device.allocate(16).unwrap();
    device.write(buffer, 4, &[1, 2, 3]).unwrap();
    assert_eq!(device.read(buffer, 4, 3).unwrap(), &[1, 2, 3]);
    assert_eq!(device.address(buffer, 4, 3).unwrap(), 0x123400004);
    device.execute(&[0]).unwrap();
    device.execute(&[0]).unwrap();
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|&&event| event == "allocate")
            .count(),
        2
    );
    drop(device);
    assert_eq!(
        &*events.borrow(),
        &["allocate", "allocate", "submit", "wait", "submit", "wait", "close", "unmap", "unmap"]
    );
}

#[test]
fn submission_and_wait_failures_close_before_any_mapping_is_released() {
    for fail in ["submit", "wait"] {
        let (mut device, events) = device(Some(fail));
        let buffer = device.allocate(16).unwrap();
        assert!(device.execute(&[0]).is_err());
        assert_eq!(events.borrow().last(), Some(&"close"));
        assert!(device.read(buffer, 0, 1).is_err());
        assert!(device.write(buffer, 0, &[1]).is_err());
        assert!(device.allocate(4).is_err());
        assert!(device.execute(&[0]).is_err());
        drop(device);
        let events = events.borrow();
        assert_eq!(events.iter().filter(|&&event| event == "close").count(), 1);
        let closed = events.iter().position(|&event| event == "close").unwrap();
        assert!(events
            .iter()
            .enumerate()
            .filter(|(_, event)| **event == "unmap")
            .all(|(index, _)| index > closed));
    }
}

#[test]
fn allocation_and_range_failures_do_not_submit() {
    let (mut failed, events) = device(Some("allocate"));
    assert!(failed.allocate(16).is_err());
    assert!(failed.execute(&[0]).is_err());
    assert!(!events.borrow().contains(&"submit"));
    let (mut device, events) = device(None);
    assert!(device.allocate(0).is_err());
    assert!(device.allocate(usize::MAX).is_err());
    let buffer = device.allocate(16).unwrap();
    assert!(device.read(buffer + 1, 0, 1).is_err());
    assert!(device.read(buffer, 15, 2).is_err());
    assert!(device.write(buffer, usize::MAX, &[1]).is_err());
    assert!(device.execute(&[]).is_err());
    assert!(!events.borrow().contains(&"submit"));
}

#[test]
fn execution_error_preserves_cleanup_failure_and_closes_only_once() {
    let (mut device, events) = device(Some("wait_and_close"));
    let error = device.execute(&[0]).unwrap_err();
    assert!(matches!(error, crate::Error::GpuCleanup { .. }));
    assert!(error.to_string().contains("wait"));
    assert!(error.to_string().contains("close"));
    drop(device);
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|&&event| event == "close")
            .count(),
        1
    );
}
