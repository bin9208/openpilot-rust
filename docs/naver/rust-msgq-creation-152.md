# Native msgq creation race (#152)

Tracks [#152](https://github.com/bin9208/openpilot-rust/issues/152), discovered in
the [#147 integration](https://github.com/bin9208/openpilot-rust/issues/147).

At `4d1601dd`, the PR GNSS job
[110237249610](https://github.com/bin9208/openpilot-rust/actions/runs/36821263060/job/110237249610)
failed because `openpilot-ubloxd` exited with `existing msgq queue has incompatible
size or type`. The subsequent executable lookup exposed that earlier exit. The
same commit's push job passed; hiding the lookup failure would not repair startup.

All three GNSS processes use the same source service capacities and an isolated
random namespace. The Rust adapter inspected queue size before the original
`msgq_new_queue` call. Original creation exposes a regular empty inode between
`open(O_CREAT)` and `ftruncate`, which the adapter rejected. A deterministic test
of that exact filesystem state reproduced the same exception in both publisher
and subscriber startup. The CI log did not retain the transient file size; the
controlled reproductions establish the incompatible adapter behavior directly.

The adapter now permits a regular zero-length file to proceed through original
msgq initialization. Existing nonzero wrong sizes and nonregular files remain
errors; capacity bounds, payload checks and publisher ownership are unchanged.
No sleeps, ignored constructor failures or transport retries were added.

## Verification

`rust/crates/msgq/tests/creation.rs` uses private per-process namespaces. Before
the fix, publisher/native-peer and subscriber-first tests failed; the initialized
size/type rejection case already passed. Afterwards all three pass. The
native-peer case exchanges a binary payload with the unchanged original C++
PubSocket/SubSocket implementation, and the subscriber-first case checks
registration across later publisher initialization. A wrong-sized existing file
is also verified to retain its bytes.

The full msgq transport, queued subscription and VisionIPC tests pass. The existing
ASan/UBSan gate now includes the creation tests and passes all four executables,
instrumenting the adapter and original peer. These are native boundary tests;
Miri does not execute this C++ implementation. Reproduction and sanitizer output
are retained under `.analysis/scratch/2026-10-01-rust-msgq-creation/`.

The original GNSS daemon fixture also passes with the repaired native binaries:
30 ubloxGnss and eight gpsLocationExternal publications match the unchanged
source parser, both receiver initializations complete, and controlled shutdown
takes about 15 ms on this host. The PTY, HTTP endpoint and GPIO files are owned
fixtures. Warning-denying Rust Clippy and formatting pass; existing upstream
C++ compiler warnings remain visible.

```sh
CARGO_INCREMENTAL=0 cargo test --manifest-path rust/Cargo.toml -p openpilot-msgq --tests --locked -j2
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 python rust/tools/check_msgq_sanitizers.py --target-dir EVIDENCE/asan-target
```

Reserve at least 25 GiB plus build growth before local builds. Parent integration
also runs the original GNSS scenario on the repaired binaries and requires
exact-SHA Actions/ARM and separate post-merge checks. This remains the temporary
original msgq transport boundary, not a full Rust transport rewrite. No device,
vehicle, private route, production namespace or C3X was accessed.

Docs-Not-Needed: source-compatible native queue initialization repair; no public
setting or production process selection change.
