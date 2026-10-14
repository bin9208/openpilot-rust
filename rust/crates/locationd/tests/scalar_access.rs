#![cfg(feature = "solver")]
use openpilot_locationd::{
    estimator::Estimator,
    types::{HandleResult, Input, Sensor},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
struct Counter;
fn record() {
    let _ = TRACK.try_with(|track| {
        if track.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}
// SAFETY: every allocation pointer and layout is delegated unchanged to System.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;

#[test]
fn rejected_primary_gyro_checks_do_not_allocate_rust_snapshots(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut estimator = Estimator::new(false)?;
    let before = estimator.kf.snapshot()?;
    ALLOCATIONS.with(|count| count.set(0));
    TRACK.with(|track| track.set(true));
    let results = std::array::from_fn::<_, 100, _>(|_| {
        estimator.handle(
            100.,
            Input::Sensor {
                kind: Sensor::Gyroscope,
                time: 100.,
                secondary: false,
                values: [11., 0., 0.],
            },
        )
    });
    TRACK.with(|track| track.set(false));
    let allocations = ALLOCATIONS.with(Cell::get);
    for result in results {
        assert_eq!(result?, HandleResult::InputInvalid);
    }
    let after = estimator.kf.snapshot()?;
    assert_eq!(after.x, before.x);
    assert_eq!(after.covariance, before.covariance);
    assert!(before.time.is_nan() && after.time.is_nan());
    assert!(estimator.logs.is_empty());
    assert_eq!(
        allocations, 0,
        "scalar gyro checks must not create owned Rust snapshots"
    );
    Ok(())
}
