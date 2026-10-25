//! Standalone allocation regression for the production proc-stat parser.
//! Compile with rustc --edition=2021; this is an owned host fixture only.
#[allow(dead_code)]
#[path = "../crates/runtime-core/src/proc_stat.rs"]
mod proc_stat;
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
// SAFETY: this single-threaded test forwards unchanged pointers/layouts to System.
// It performs no allocation while counting and never changes ownership semantics.
#[allow(unsafe_code)]
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
fn main() {
    let mut fields = vec!["0"; 50];
    fields[0] = "S";
    fields[1] = "1";
    fields[11] = "1234";
    fields[12] = "5678";
    fields[36] = "6";
    let input = format!("12 (owned worker) {}", fields.join(" "));
    let count = 1_000;
    ALLOCATIONS.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    for _ in 0..count {
        let parsed = std::hint::black_box(proc_stat::ProcessStat::parse(&input)).unwrap();
        assert_eq!(
            (
                parsed.pid,
                parsed.user_ticks,
                parsed.system_ticks,
                parsed.processor
            ),
            (12, 1234, 5678, 6)
        );
        assert_eq!(parsed.name, "owned worker");
    }
    TRACK.store(false, Ordering::Relaxed);
    let observed = ALLOCATIONS.load(Ordering::Relaxed);
    println!(
        "{{\"records\":{count},\"allocation_calls\":{observed},\"name_allocations\":{count}}}"
    );
    assert_eq!(
        observed, count,
        "only the owned process name should allocate"
    );
}
