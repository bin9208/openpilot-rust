# Messaging subscriber update allocation candidate (#431)

Tracking: [#431](https://github.com/bin9208/openpilot-rust/issues/431).

Exact-source cloud acceptance passed at
`2d8d027932dc880bd0acd00288b256ac8d7f1b89` in
[selfdrived job 118875506156](https://github.com/bin9208/openpilot-rust/actions/runs/39494388960/job/118875506156).
The parent downloaded artifact `12296943884` (`rust-selfdrived-host-evidence`,
ZIP SHA-256 `e6b5eb8968a4d16b6d29182ddf878cd3106e055a8cca817b2e180c44b7eaeb7f`)
and independently read the complete old/current receipts, input library hashes,
probe executable hashes and successful comparison. Production messaging tests,
original/native state comparisons and strict Clippy passed in the same job.

| Case | Updates | Old calls / requested bytes | Current calls / requested bytes | Identical semantic fingerprint |
| --- | ---: | ---: | ---: | --- |
| empty | 1000 | 1000 / 8000 | 1000 / 8000 | `62330b62e642d165` |
| single | 1000 | 7000 / 920000 | 6000 / 896000 | `048ddba0275cfc65` |
| two topics | 1000 | 10000 / 1472000 | 9000 / 1424000 | `8b768745c115d2f1` |
| malformed second message | 1 | 0 / 0 | 0 / 0 | `5e44c682e4714dfd` |

Both ordinary nonempty paths save 1,000 Rust allocator calls over 1,000
updates, with 24,000 and 48,000 fewer requested bytes respectively. Empty and
malformed controls remain unchanged. The probe source SHA-256 is
`765a58604ce8a647c473690e87e50e13c2606c0fcb13f380720df4e509fe5458`.
This is host allocator evidence, not device CPU, latency or peak-memory proof.
The overall workflow still has other failed jobs; dev integration and user
device acceptance remain outstanding.

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
the existing original/native state comparison on the current exact SHA.
These gates passed at the source revision recorded above.

The probe counts Rust host allocator calls and requested bytes only while the
actual `SubMaster::update(Duration::ZERO)` processes empty, one-topic and
two-topic batches. It warms each path before measuring. Packet construction,
publication, semantic fingerprinting, assertions and printing are outside the
counted window. A separate actual malformed two-topic batch must fail before
changing frame or topic fields. The receipt excludes C++ transport allocations
and is not a peak-memory, device CPU or timing measurement. No vehicle behavior
is claimed.

Docs-Not-Needed: this is an internal bounded source and validation record.
