//! Standalone host allocation/semantic regression for production State::snapshot.
//! Link a freshly built openpilot_wifi rlib; no D-Bus, NetworkManager, or device.
//! --baseline records current behavior; --require-improvement enforces removal
//! of exactly the discarded deep clones, without a sorting/allocation redesign.
use openpilot_wifi::{ConnectStatus, SecurityType, WifiState};
#[path = "../crates/wifi/tests/snapshot/cases.rs"]
mod snapshot_cases;
use snapshot_cases::{immutable_fingerprint, legacy_snapshot, network, seed, semantic_checks};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
struct Count;
fn record(bytes: usize) {
    if TRACK.load(Ordering::Relaxed) {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(bytes, Ordering::Relaxed);
    }
}
// SAFETY: this single-threaded probe forwards all pointers/layouts unchanged to
// System. Its counters allocate nothing. Bytes means total requested allocation
// bytes (including full new realloc sizes), not peak/live memory or device CPU.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Allocation {
    calls: usize,
    bytes: usize,
}
fn measure(mut operation: impl FnMut()) -> Allocation {
    CALLS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    operation();
    TRACK.store(false, Ordering::Relaxed);
    Allocation {
        calls: CALLS.load(Ordering::Relaxed),
        bytes: BYTES.load(Ordering::Relaxed),
    }
}
fn discarded_clones(state: &openpilot_wifi::state::State) {
    black_box((
        state.snapshot.networks.clone(),
        state.snapshot.saved_ssids.clone(),
        state.snapshot.connecting_to_ssid.clone(),
        state.snapshot.connected_ssid.clone(),
    ));
}
fn allocation_case(size: usize, iterations: usize, require_improvement: bool) {
    let mut state = seed();
    state.snapshot.wifi_state = WifiState {
        ssid: Some("active-not-visible".into()),
        status: ConnectStatus::Connected,
    };
    state.snapshot.networks = (0..size)
        .map(|i| {
            network(
                &format!("fixture-network-{:04}-mixed-É가", size - i),
                (i % 101) as i32,
                SecurityType::Wpa2,
                i % 11 == 0,
            )
        })
        .collect();
    state.connections = (0..size / 2)
        .map(|i| (format!("saved-{i:04}"), format!("/connection/{i}")))
        .collect();
    state.snapshot.saved_ssids = (0..size / 3)
        .map(|i| format!("discarded-cache-{i:04}"))
        .collect();
    let before = immutable_fingerprint(&state);
    let expected = legacy_snapshot(&state);
    assert_eq!(state.snapshot(), expected);
    let legacy = measure(|| {
        for _ in 0..iterations {
            black_box(legacy_snapshot(black_box(&state)));
        }
    });
    let candidate = measure(|| {
        for _ in 0..iterations {
            black_box(state.snapshot());
        }
    });
    let waste = measure(|| {
        for _ in 0..iterations {
            discarded_clones(black_box(&state));
        }
    });
    println!("{{\"networks\":{size},\"iterations\":{iterations},\"legacy_calls\":{},\"legacy_bytes\":{},\"candidate_calls\":{},\"candidate_bytes\":{},\"discarded_clone_calls\":{},\"discarded_clone_bytes\":{}}}",
        legacy.calls, legacy.bytes, candidate.calls, candidate.bytes, waste.calls, waste.bytes);
    assert_eq!(immutable_fingerprint(&state), before);
    assert_eq!(state.snapshot(), expected);
    if require_improvement {
        assert_eq!(
            candidate.calls.checked_add(waste.calls),
            Some(legacy.calls),
            "remove exactly the discarded clone calls"
        );
        assert_eq!(
            candidate.bytes.checked_add(waste.bytes),
            Some(legacy.bytes),
            "remove exactly the discarded clone bytes"
        );
    } else {
        assert_eq!(
            candidate, legacy,
            "baseline no longer matches sealed old algorithm"
        );
    }
}
fn main() {
    let mode = std::env::args()
        .nth(1)
        .expect("--baseline or --require-improvement");
    assert!(matches!(
        mode.as_str(),
        "--baseline" | "--require-improvement"
    ));
    let cases = semantic_checks();
    println!("{{\"semantic_cases\":{cases},\"source_immutability\":true}}");
    for size in [8, 1024] {
        allocation_case(size, 8, mode == "--require-improvement");
    }
}
