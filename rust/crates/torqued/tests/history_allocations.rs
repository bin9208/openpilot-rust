//! Allocation contract for a warmed bounded history; numerical policy is unchanged.
use openpilot_torqued::history::{History, Input};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

static TRACK: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
struct Count;
fn record() {
    if TRACK.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    }
}
// SAFETY: this dedicated single-test process forwards pointers/layouts unchanged
// to System; counting uses nonallocating atomics and does not alter ownership.
unsafe impl GlobalAlloc for Count {
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
static ALLOCATOR: Count = Count;

#[test]
fn warmed_wrapped_history_allocates_only_queries_and_four_results() {
    let mut history = History::default();
    // Advance beyond capacity so VecDeque's logical order crosses its split.
    for index in 0..250 {
        let time = f64::from(index) * 0.05;
        for input in [
            Input::Control { time, active: true },
            Input::Output { time, torque: 0.2 },
            Input::State {
                time,
                speed: 20.,
                pressed: false,
            },
        ] {
            assert!(history.handle(input).unwrap().is_none());
        }
    }
    let pose = Input::Pose {
        time: 12.45,
        roll: 0.,
        angular: [0., 0., 0.1],
        valid: true,
    };
    assert_eq!(history.handle(pose.clone()).unwrap(), Some([-0.2, 2.]));
    ALLOCATIONS.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    for _ in 0..100 {
        assert_eq!(history.handle(pose.clone()).unwrap(), Some([-0.2, 2.]));
    }
    TRACK.store(false, Ordering::Relaxed);
    assert_eq!(
        ALLOCATIONS.load(Ordering::Relaxed),
        500,
        "the engagement query and four outputs allocate; bounded history columns must not"
    );
    history.clear_samples();
    assert!(history.handle(pose).unwrap().is_none());
}
