//! Host allocation regression for the actual JSON view library.
use openpilot_log_format::JsonValue;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
static TRACK: AtomicBool = AtomicBool::new(false);
static COUNT: AtomicUsize = AtomicUsize::new(0);
struct Counter;
fn record() {
    if TRACK.load(Ordering::Relaxed) {
        COUNT.fetch_add(1, Ordering::Relaxed);
    }
}
// SAFETY: forwards all pointers and layouts unchanged to the system allocator.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
fn main() {
    let values = ["[1,2,3]", "{\"x\":1}", "null", "false", "42", "1.5"]
        .map(|source| JsonValue::parse(source).unwrap());
    TRACK.store(true, Ordering::Relaxed);
    for _ in 0..1_000 {
        for value in &values {
            assert!(!std::hint::black_box(value).text_eq(""));
            assert!(std::hint::black_box(value).to_utf8().is_none());
        }
    }
    TRACK.store(false, Ordering::Relaxed);
    let count = COUNT.load(Ordering::Relaxed);
    println!("JSON nonstring accessor allocations: {count}");
    assert_eq!(
        count, 0,
        "scalar access must not materialize composite views"
    );
}
