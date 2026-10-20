# Messaging subscriber update allocation candidate (#431)

Tracking: [#431](https://github.com/bin9208/openpilot-rust/issues/431).

At baseline `7709c2e3c455cefba0ff2a72dffad7c22375e49a`,
`SubMaster::update` converted the owned `Vec<QueuedMessage>` from
`MultiSubscriber::receive` into a second `Vec<Vec<u8>>` before passing it to
`State::update_with_clock`. That conversion discarded message indexes. Rust may
reuse the incoming vector's allocation for `into_iter().map().collect()`, so
source inspection alone does not establish an allocation reduction. `State`
separately collected fully parsed Cap'n Proto readers before changing frame or topic state; that transactional parse
boundary is retained.

The source change lets `State::update_with_clock` accept an iterator of byte
buffers and passes borrowed slices from the received messages. Existing
`State::update` and callers with `&[Vec<u8>]` keep their API and behavior. The
new state test covers an accepted two-topic batch and a malformed second
message that leaves both topics and frame unchanged. Existing isolated IPC
tests cover publication, timeout, conflation and single-message parse error.

For a numeric receipt, use GitHub Actions with two isolated checkouts, one at
the exact baseline SHA above and one at the change SHA. Check disk reserve
before builds. Build `openpilot-messaging` with native enabled in each checkout
using separate target directories. Compile the **same**
`rust/tools/messaging_update_allocation_probe.rs` from the change checkout with
standalone `rustc --edition=2021`, passing explicit `--extern` paths for that
checkout's single messaging, cereal and Cap'n Proto rlibs and `-L dependency`
for that checkout's target `debug/deps`. Fail if an rlib selection is absent or
ambiguous. Never use a new messaging artifact for the old run. The probe cannot
be built as a messaging Cargo example without changing the package's inherited
`unsafe_code = "forbid"` lint: its exact counter needs an `unsafe impl
GlobalAlloc`, so the standalone probe follows the repository's existing
allocation-probe pattern. Run each probe
once with its private `/dev/shm` namespaces and retain its receipt, executable
hash, source SHA and build logs. Run the candidate probe's
`--compare OLD_RECEIPT CURRENT_RECEIPT` mode: it requires the exact four-row
roster, 1,000 measured updates per ordinary row, matching semantic
fingerprints, unchanged empty-update allocation counts/bytes, and strictly
lower Rust allocation calls **and** requested bytes in both single- and
two-topic paths. The source-level prediction of 1,000 fewer calls over 1,000
single-topic updates is a hypothesis, not a gate or result; a failed improvement
gate means the optimization must be reconsidered, not that thresholds should be
relaxed. Run the complete messaging package tests, strict all-target Clippy and
the existing original/native state comparison on the current exact SHA. None
of these cloud checks has run during this source preparation.

The probe counts Rust host allocator calls and requested bytes only while the
actual `SubMaster::update(Duration::ZERO)` processes empty, one-topic and
two-topic batches. It warms each path before measuring. Packet construction,
publication, semantic fingerprinting, assertions and printing are outside the
counted window. A separate actual malformed two-topic batch must fail before
changing frame or topic fields. The receipt excludes C++ transport allocations
and is not a peak-memory, device CPU or timing measurement. All final outputs
remain cloud pending; no vehicle behavior is claimed.

Docs-Not-Needed: this is an internal bounded source and validation record.
