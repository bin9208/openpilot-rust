# M1 validation ledger

Tracking: [issue #5](https://github.com/bin9208/openpilot-rust/issues/5).
The user will execute the first C3X probe. No device connection, installation,
production process replacement or vehicle test is part of this preparation.

## Collector and wire format

The collector adds eleven synthetic procfs tests to the twelve existing core
tests. They cover clock/page units, signed counters, command-line decoding,
vanished and malformed processes, PID reuse, cache eviction, the twenty-cycle
smaps refresh, small-process exclusion and rollup fallback.
An additional FIFO-synchronized race reproduces PID reuse during metadata
collection. Rechecking PID/start ticks/name before publishing prevents mixed
identity and evicts the affected cache entries. This test failed before the fix.
Independent review also reproduced loss of valid smaps counters when a mapping
filename contains non-UTF-8 bytes. Byte-oriented parsing preserves those
counters, matching source Python; the regression failed with PSS=0 before the
fix and passes with PSS=51200. The Python oracle now covers both smaps_rollup
and fallback smaps, including that filename, for 22 cycles each.

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

## Params raw storage

The Rust registry is generated from the original params.h/params_keys.h and
the canonical LongitudinalPersonality enum. Unknown syntax, flags, types,
defaults and duplicate keys fail generation. Raw reads distinguish a missing
file (`None`) from a present empty file (`Some([])`); Python callers that treat
both as missing must preserve that policy in their eventual adapter.

Synthetic storage tests pass for binary/empty/missing values, key/prefix
validation, existing native symlinks, clear masks and unknown-file cleanup,
and concurrent atomic writers. The original C++ Params and util translation
units are compiled as the reference. Only their logging sink is replaced by
stderr; storage, locking, metadata and filesystem behavior are original code.
Full catalogs match. Both implementations read the other's 256 KiB binary
value, observe empty writes, remove each other's values, honor clear flags,
and block on the same flock before publishing. The native constructor also
creates the namespace consumed by Rust.

```sh
cargo build -p openpilot-params --example store --locked
python tools/check_params_reference.py
```

For rootless Cap'n Proto installations, supply `--capnp-prefix /path/to/usr`.

## Bounded executable and target candidate

Four CLI tests cover help/invalid arguments, production namespace refusal,
bounded canonical file output, overwrite refusal, broken pipes and the actual
temporary-Params/msgq self-test. The executable timestamps each sample before
collection using CLOCK_MONOTONIC, matching Python messaging.new_message's
time.monotonic clock and call order. Publication uses the source service's
10 MiB queue; the reference check asserts the Rust constant against services.py.
The bridge rejects an incompatible existing queue size before ftruncate and
uses an exclusive flock to prevent two Rust publishers on one endpoint.
Native consumers/publishers do not participate in that Rust publisher lock;
the mandatory fresh isolated namespace protects the production publisher.

The real Python cereal.messaging consumer received all ten host messages at
the default 2000 ms cadence. The probe checks Event validity, increasing time,
nonempty CPU/memory/process data and the producer's PID. Its report contains
counts and timing, never raw process identities or command lines.

Rust 1.94.0 with cargo-zigbuild 0.23.4/Zig 0.16.0 builds the static aarch64-musl
candidate. ELF inspection confirms AArch64, no interpreter and no dynamic
dependencies. QEMU 8.2 executes its temporary Params and msgq self-test, then
sends three synthetic procLog messages to the real x86_64 Python/msgq consumer.
This checks cross-architecture shared-memory/wire layout, not AGNOS behavior.

Trying the native device probe under QEMU exposed QEMU's synthetic 44-field
self stat, which both source Python and Rust intentionally reject as truncated.
A procfs symlink also resolves to that emulated view. The real device probe's
producer-PID assertion remains intact; a separate emulator check uses complete
synthetic procfs fixtures. No parser or device acceptance check was weakened.
See QEMU's [open_self_stat implementation](https://gitlab.com/qemu-project/qemu/-/blob/v8.2.2/linux-user/syscall.c).

CI retains the generic GNU aarch64 build and adds the static candidate with
SOURCE_COMMIT and SHA256SUMS. The candidate's QEMU result explicitly records
`device_validation: not_run`. Required pre-merge and separate post-merge checks
must pass at the exact reviewed SHA. C3X execution remains with the user under
the [probe procedure](c3x-probe.md); issues #1 and #5 remain open.
