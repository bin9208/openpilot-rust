//! Standalone host allocation/semantic regression for production State::snapshot.
//! Link a freshly built openpilot_wifi rlib; no D-Bus, NetworkManager, or device.
//! --baseline records current behavior; --require-improvement enforces removal
//! of exactly the discarded deep clones, without a sorting/allocation redesign.
use openpilot_wifi::{
    state::State, ConnectStatus, MeteredType, Network, SecurityType, Snapshot, WifiState,
};
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
// Frozen pre-optimization algorithm. It is used only for native before/after
// allocation/output comparison, not as an independent Python policy oracle.
#[inline(never)]
fn legacy_snapshot(state: &State) -> Snapshot {
    let mut result = state.snapshot.clone();
    result.networks = state.networks();
    result.saved_ssids = state
        .connections
        .iter()
        .map(|(ssid, _)| ssid.clone())
        .collect();
    result.tethering_active = result.wifi_state.ssid.as_deref() == Some(&state.tethering_ssid);
    result.connecting_to_ssid = if result.wifi_state.status == ConnectStatus::Connecting {
        result.wifi_state.ssid.clone()
    } else {
        None
    };
    result.connected_ssid = if result.wifi_state.status == ConnectStatus::Connected {
        result.wifi_state.ssid.clone()
    } else {
        None
    };
    result
}
fn discarded_clones(state: &State) {
    black_box((
        state.snapshot.networks.clone(),
        state.snapshot.saved_ssids.clone(),
        state.snapshot.connecting_to_ssid.clone(),
        state.snapshot.connected_ssid.clone(),
    ));
}
fn network(ssid: &str, strength: i32, security_type: SecurityType, is_tethering: bool) -> Network {
    Network {
        ssid: ssid.into(),
        strength,
        security_type,
        is_tethering,
    }
}
fn seed() -> State {
    let mut state = State::new(Some("검증문자more"));
    state.snapshot.ipv4_address = "192.0.2.123".into();
    state.snapshot.tethering_password = "private-host-fixture-only".into();
    state.snapshot.current_network_metered = MeteredType::Yes;
    // Stale cache values must be replaced, never copied into returned fields.
    state.snapshot.saved_ssids = vec!["obsolete-saved-a".into(), "obsolete-saved-b".into()];
    state.snapshot.connecting_to_ssid = Some("obsolete-connecting".into());
    state.snapshot.connected_ssid = Some("obsolete-connected".into());
    state.snapshot.tethering_active = true;
    state.device = Some("/private/device".into());
    state.active = false;
    state.ipv4_forward = true;
    state.last_scan = 123.5;
    state.epoch = 41;
    state
        .events
        .push(openpilot_wifi::Event::Forgotten("original-event".into()));
    state
}
fn immutable_fingerprint(state: &State) -> String {
    format!(
        "{:?}|{:?}|{:?}|{}|{:?}|{}|{}|{}|{}",
        state.snapshot,
        state.connections,
        state.events,
        state.tethering_ssid,
        state.device,
        state.active,
        state.ipv4_forward,
        state.last_scan,
        state.epoch
    )
}
fn semantic_checks() -> usize {
    let mut state = seed();
    state.connections = vec![
        ("B".into(), "/b".into()),
        ("b".into(), "/lower-b".into()),
        ("saved-only".into(), "/not-visible".into()),
    ];
    state.snapshot.networks = vec![
        network("z-last", 100, SecurityType::Open, false),
        network("b", 20, SecurityType::Wpa, false),
        network("B", 20, SecurityType::Wpa2, false),
        network("A", 20, SecurityType::Wpa3, false),
        network("a", 20, SecurityType::Unsupported, false),
        network("É", 80, SecurityType::Open, false),
        network("é", 80, SecurityType::Wpa, false),
        network("가", 80, SecurityType::Wpa2, false),
        network(&state.tethering_ssid, 1, SecurityType::Wpa3, true),
    ];
    // Hand-ordered policy: saved first, strength descending, lowercase ascending;
    // b/B, A/a and É/é ties preserve their input order. No comparator change.
    let base_order = [1, 2, 0, 5, 6, 7, 3, 4, 8];
    let mut cases = 0;
    for metered in [MeteredType::Unknown, MeteredType::Yes, MeteredType::No] {
        state.snapshot.current_network_metered = metered;
        for status in [
            ConnectStatus::Disconnected,
            ConnectStatus::Connecting,
            ConnectStatus::Connected,
        ] {
            for ssid in [
                None,
                Some(String::new()),
                Some("B".into()),
                Some(state.tethering_ssid.clone()),
                Some("not-visible".into()),
            ] {
                state.snapshot.wifi_state = WifiState {
                    ssid: ssid.clone(),
                    status,
                };
                let before = immutable_fingerprint(&state);
                let mut order = base_order.to_vec();
                if let Some(index) = order
                    .iter()
                    .position(|i| Some(&state.snapshot.networks[*i].ssid) == ssid.as_ref())
                {
                    let active = order.remove(index);
                    order.insert(0, active);
                }
                let expected = Snapshot {
                    networks: order
                        .into_iter()
                        .map(|i| state.snapshot.networks[i].clone())
                        .collect(),
                    wifi_state: WifiState {
                        ssid: ssid.clone(),
                        status,
                    },
                    ipv4_address: "192.0.2.123".into(),
                    current_network_metered: metered,
                    connecting_to_ssid: if status == ConnectStatus::Connecting {
                        ssid.clone()
                    } else {
                        None
                    },
                    connected_ssid: if status == ConnectStatus::Connected {
                        ssid.clone()
                    } else {
                        None
                    },
                    tethering_password: "private-host-fixture-only".into(),
                    tethering_active: ssid.as_deref() == Some(&state.tethering_ssid),
                    saved_ssids: vec!["B".into(), "b".into(), "saved-only".into()],
                };
                let mut actual = state.snapshot();
                assert_eq!(actual, expected);
                assert_eq!(actual, legacy_snapshot(&state));
                // Returned owned fields must not alias/mutate source storage.
                actual.networks[0].ssid.push_str("-returned-only");
                actual.saved_ssids[0].clear();
                actual.wifi_state.ssid = Some("returned-only".into());
                actual.ipv4_address.clear();
                actual.tethering_password.clear();
                actual.connecting_to_ssid = None;
                actual.connected_ssid = None;
                assert_eq!(immutable_fingerprint(&state), before);
                assert_eq!(state.snapshot(), expected);
                cases += 1;
            }
        }
    }
    let empty = State::new(None);
    assert_eq!(empty.snapshot(), Snapshot::default());
    cases + 1
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
