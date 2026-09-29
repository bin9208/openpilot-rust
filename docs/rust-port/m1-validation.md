# M1 validation ledger

Tracking: [issue #5](https://github.com/bin9208/openpilot-rust/issues/5).
The user will execute the first C3X probe. No device connection, installation,
production process replacement or vehicle test is part of this preparation.

## Collector and wire format

The collector adds nine synthetic procfs tests to the twelve existing core
tests. They cover clock/page units, signed counters, command-line decoding,
vanished and malformed processes, PID reuse, cache eviction, the twenty-cycle
smaps refresh, small-process exclusion and rollup fallback.

The cereal crate generates bindings from the complete original log, car,
custom and deprecated schemas. capnpc 0.27.0 emits unused generic parameters
on the two annotation functions for Map and Map.Entry. The build removes
those parameters and their call-site arguments only after checking that both
functions have the expected annotation-free panic body. A changed generator
shape fails the build for review. No schema or lint is disabled.

Three wire tests verify canonical Event decoding, signed values, empty
snapshots and checked narrowing. `rust/tools/check_proclog_reference.py`
loads the actual Python collector functions through the AST, redirects only
their filesystem boundary to a synthetic proc tree, and compares every
decoded Event/procLog field using pycapnp. All fields match for 22 successive
cycles, including an altered smaps value that becomes visible at cycle 20.
PID-reuse invalidation is an intentional correction separately covered by
Rust tests; the original Python PID-only cache is not the oracle for it.

Local Ubuntu commands, Rust 1.94.0 and Cap'n Proto 1.0.1:

```sh
cd rust
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p openpilot-proclogd --example reference_trace --locked
cd ..
uv run rust/tools/check_proclog_reference.py
```

These are host compatibility checks, not C3X execution or CPU measurements.

## Native msgq boundary

The CXX bridge owns a value-initialized native `msgq_queue_t` and calls the
original msgq implementation. Using the lower-level queue API avoids the
inherited socket factory's uninitialized queue cleanup on failed connect.
Rust receives owned byte copies; transport handles cannot move across threads.
Empty/oversized payloads are rejected before native assertions. Constructors
require an existing `rust-probe-NAME` namespace and reject fake transport.

Real shared-memory tests passed for timeouts, payload preservation, conflation,
publisher reconnect, repeated subscription cleanup and production-namespace
refusal. A separate executable built with the original PubSocket/SubSocket
API echoes a 200,000-byte binary payload from Rust back to Rust.

The same tests passed with every C++ transport/bridge/peer translation unit
instrumented by AddressSanitizer and UndefinedBehaviorSanitizer, leak checking
enabled. Rust code was not sanitizer-instrumented. The native FFI feature is
named `native-skip-miri` because Miri cannot execute this native transport.
The first sanitizer launch stopped at runtime load ordering; rerunning the
built test with libasan preloaded passed without sanitizer findings.

```sh
CXXFLAGS='-fsanitize=address,undefined -fno-omit-frame-pointer' \
RUSTFLAGS='-C link-arg=-lasan -C link-arg=-lubsan' \
CARGO_TARGET_DIR=/tmp/rust-msgq-asan cargo test -p openpilot-msgq --test transport --no-run --locked
LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libasan.so.8 \
ASAN_OPTIONS=detect_leaks=1 UBSAN_OPTIONS=halt_on_error=1 \
/tmp/rust-msgq-asan/debug/deps/transport-<hash> --nocapture
```
