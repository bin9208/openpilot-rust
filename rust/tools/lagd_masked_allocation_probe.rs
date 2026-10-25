//! Standalone cloud before/after probe for the two masked input vectors.
//! Link the same source against old and current openpilot_lagd libraries.
use openpilot_lagd::correlation::Correlator;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    path::Path,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
struct Count;

fn record(size: usize) {
    if TRACK.load(Ordering::Relaxed) {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size, Ordering::Relaxed);
    }
}

// SAFETY: The single-test probe forwards all pointers and layouts to System.
// Requested bytes include new realloc sizes; these are not peak/live memory.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Count = Count;

const ITERATIONS: usize = 16;

#[derive(Debug)]
struct Record {
    length: usize,
    iterations: usize,
    calls: usize,
    bytes: usize,
    fingerprint: u64,
}

fn records(path: &Path) -> [Record; 2] {
    let content = std::fs::read_to_string(path).unwrap();
    let lines: Vec<_> = content.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "expected exactly two allocation records: {}",
        path.display()
    );
    let mut parsed = lines.into_iter().map(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 6, "malformed allocation record: {line}");
        assert_eq!(fields[0], "MASKED_ALLOC", "unexpected record kind");
        let number =
            |field: &str, key: &str| -> usize { field.strip_prefix(key).unwrap().parse().unwrap() };
        let fingerprint = fields[5].strip_prefix("fingerprint=").unwrap();
        assert_eq!(fingerprint.len(), 16, "invalid fingerprint width");
        assert!(fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
        Record {
            length: number(fields[1], "length="),
            iterations: number(fields[2], "iterations="),
            calls: number(fields[3], "calls="),
            bytes: number(fields[4], "bytes="),
            fingerprint: u64::from_str_radix(fingerprint, 16).unwrap(),
        }
    });
    let result = [parsed.next().unwrap(), parsed.next().unwrap()];
    for (row, length) in result.iter().zip([64, 128]) {
        assert_eq!(row.length, length, "wrong allocation case order");
        assert_eq!(row.iterations, ITERATIONS, "wrong iteration count");
    }
    result
}

fn compare(old: &Path, current: &Path) {
    for (before, after) in records(old).iter().zip(records(current).iter()) {
        assert_eq!(
            before.fingerprint, after.fingerprint,
            "numeric output drift at length {}",
            before.length
        );
        assert_eq!(
            before.calls.checked_sub(after.calls),
            Some(2 * ITERATIONS),
            "allocation call delta at length {}",
            before.length
        );
        assert_eq!(
            before.bytes.checked_sub(after.bytes),
            Some(2 * before.length * 8 * ITERATIONS),
            "requested byte delta at length {}",
            before.length
        );
        println!(
            "MASKED_COMPARE length={} calls_saved={} bytes_saved={} fingerprint={:016x}",
            before.length,
            before.calls - after.calls,
            before.bytes - after.bytes,
            before.fingerprint
        );
    }
}

fn measure() {
    for length in [64, 128] {
        let expected: Vec<_> = (0..length)
            .map(|index| ((index * 7) % 29) as f64 * 0.125 - 1.5)
            .collect();
        let actual: Vec<_> = (0..length)
            .map(|index| ((index * 11 + 3) % 31) as f64 * 0.0625 - 0.75)
            .collect();
        let mask: Vec<_> = (0..length).map(|index| index % 5 != 0).collect();
        let mut correlator = Correlator::new(256).unwrap();
        let warm = correlator.masked(&expected, &actual, &mask).unwrap();
        assert_eq!(warm.len(), 256);

        CALLS.store(0, Ordering::Relaxed);
        BYTES.store(0, Ordering::Relaxed);
        TRACK.store(true, Ordering::Relaxed);
        let mut last = Vec::new();
        for _ in 0..ITERATIONS {
            last = black_box(
                correlator
                    .masked(black_box(&expected), black_box(&actual), black_box(&mask))
                    .unwrap(),
            );
        }
        TRACK.store(false, Ordering::Relaxed);
        assert_eq!(last, warm, "warm and measured output differ");
        let fingerprint = last.iter().fold(0xcbf29ce484222325u64, |hash, value| {
            (hash ^ value.to_bits()).wrapping_mul(0x100000001b3)
        });
        println!(
            "MASKED_ALLOC length={length} iterations={ITERATIONS} calls={} bytes={} fingerprint={fingerprint:016x}",
            CALLS.load(Ordering::Relaxed),
            BYTES.load(Ordering::Relaxed),
        );
    }
}

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    match arguments.as_slice() {
        [] => measure(),
        [flag, old, current] if flag == std::ffi::OsStr::new("--compare") => {
            compare(Path::new(old), Path::new(current));
        }
        _ => panic!("usage: lagd_masked_allocation_probe [--compare OLD_LOG CURRENT_LOG]"),
    }
}
