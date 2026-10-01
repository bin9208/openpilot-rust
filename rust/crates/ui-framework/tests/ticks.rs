#![cfg(feature = "native")]
use openpilot_ui_framework::application::{Tick, TickRegistry};
use std::{cell::RefCell, rc::Rc};
#[test]
fn tick_can_remove_pending_work_and_append_new_work_during_the_same_frame() {
    let registry = TickRegistry::default();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let log = calls.clone();
    let removed = Tick::new(move || log.borrow_mut().push("removed"));
    let log = calls.clone();
    let added = Tick::new(move || log.borrow_mut().push("added"));
    let entries = registry.clone();
    let log = calls.clone();
    let remove = removed.clone();
    let add = added.clone();
    let first = Tick::new(move || {
        log.borrow_mut().push("first");
        entries.remove(&remove);
        entries.add(add.clone());
    });
    registry.add(first.clone());
    registry.add(first);
    registry.add(removed);
    registry.run().unwrap();
    assert_eq!(*calls.borrow(), ["first", "added"]);
    registry.run().unwrap();
    assert_eq!(*calls.borrow(), ["first", "added", "first", "added"]);
}
