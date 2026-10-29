//! Standalone cloud-only allocation comparison for the production cookie header helper.
//! The legacy side is the exact previous formatting algorithm, not an old LPA build
//! or an independent cookie-selection oracle. Compile this file directly with rustc.

#[path = "../crates/lpa/src/http/cookies.rs"]
mod cookies;

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

#[derive(Clone, Copy, Default)]
struct Allocation {
    calls: usize,
    bytes: usize,
}
thread_local! {
    static TRACK: Cell<Option<Allocation>> = const { Cell::new(None) };
}
struct Count;
fn record(bytes: usize) {
    let _ = TRACK.try_with(|track| {
        if let Some(mut count) = track.get() {
            count.calls += 1;
            count.bytes += bytes;
            track.set(Some(count));
        }
    });
}
// SAFETY: pointers and layouts are passed unchanged to System. The thread-local
// counters do not allocate; bytes count requested alloc/realloc sizes, not peak RAM.
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Count = Count;

fn measure(mut operation: impl FnMut()) -> Allocation {
    TRACK.with(|track| track.set(Some(Allocation::default())));
    operation();
    TRACK.with(|track| track.replace(None).unwrap())
}

#[inline(never)]
fn legacy<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}
fn borrowed(pairs: &[(String, String)]) -> impl Iterator<Item = (&str, &str)> + '_ {
    pairs
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
}
fn semantic_checks() {
    let cases: &[(&[(&str, &str)], &str)] = &[
        (&[], ""),
        (&[("", "")], "="),
        (&[("a", "b"), ("c", "d")], "a=b; c=d"),
        (&[("same", "1"), ("same", "2")], "same=1; same=2"),
        (&[("é가", "λ"), ("x", "a=b; c")], "é가=λ; x=a=b; c"),
    ];
    for (pairs, expected) in cases {
        assert_eq!(legacy(pairs.iter().copied()), *expected);
        assert_eq!(cookies::header(pairs.iter().copied()), *expected);
    }
}
fn compare(size: usize, require_improvement: bool) {
    let pairs: Vec<(String, String)> = (0..size)
        .map(|index| {
            (
                format!("cookie-{index:04}"),
                format!("value-{index:04}-é가;={}", index % 7),
            )
        })
        .collect();
    let expected = legacy(borrowed(&pairs));
    assert_eq!(cookies::header(borrowed(&pairs)), expected);
    for _ in 0..4 {
        black_box(legacy(black_box(borrowed(&pairs))));
        black_box(cookies::header(black_box(borrowed(&pairs))));
    }
    const ITERATIONS: usize = 64;
    let before = measure(|| {
        for _ in 0..ITERATIONS {
            black_box(legacy(black_box(borrowed(&pairs))));
        }
    });
    let after = measure(|| {
        for _ in 0..ITERATIONS {
            black_box(cookies::header(black_box(borrowed(&pairs))));
        }
    });
    assert_eq!(cookies::header(borrowed(&pairs)), expected);
    println!(
        "{{\"cookies\":{size},\"iterations\":{ITERATIONS},\"header_bytes\":{},\"legacy_calls\":{},\"legacy_bytes\":{},\"candidate_calls\":{},\"candidate_bytes\":{}}}",
        expected.len(), before.calls, before.bytes, after.calls, after.bytes
    );
    if require_improvement {
        assert!(
            after.calls < before.calls,
            "{size} cookies: allocation calls did not fall"
        );
        assert!(
            after.bytes < before.bytes,
            "{size} cookies: requested bytes did not fall"
        );
    }
}
fn main() {
    let mut arguments = std::env::args();
    let program = arguments.next().unwrap();
    let require_improvement = match (arguments.next(), arguments.next()) {
        (None, None) => false,
        (Some(mode), None) if mode == "--report" => false,
        (Some(ref mode), None) if mode == "--require-improvement" => true,
        _ => panic!("usage: {program} [--report|--require-improvement]"),
    };
    semantic_checks();
    for size in [8, 1024] {
        compare(size, require_improvement);
    }
}
