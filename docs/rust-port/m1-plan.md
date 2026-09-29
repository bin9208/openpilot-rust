# Rust IPC, Params and procLog implementation plan

> **For agentic workers:** Use superpowers:executing-plans for native implementation, with one independent whole-branch review. User authorized continuous work up to the device-test boundary; do not request stage approval again.

**Goal:** Prepare an independently runnable Rust procLog producer and storage/IPC boundaries for the user's first C3X test from dev.

**Architecture:** Keep the full existing cereal schemas and native msgq transport. Generate Rust Cap'n Proto bindings, isolate native messaging behind a small CXX ownership boundary, and implement procfs collection and Params storage in safe Rust. The first device probe uses an isolated msgq namespace and temporary Params store, never the production publisher or vehicle settings.

**Tech stack:** Rust 1.94.0; capnp/capnpc 0.27; CXX 1.0; rustix 1.1; tempfile 3; existing C++ msgq; Python and native C++ test oracles; pinned Cargo.lock.

**Spec:** [design.md](design.md), [issue #5](https://github.com/bin9208/openpilot-rust/issues/5). Model feasibility is independently tracked in [#6](https://github.com/bin9208/openpilot-rust/issues/6).

## Global constraints

- Work only in bin9208/openpilot-rust, from dev, through issue branches and required PR/post-merge checks.
- Preserve production process selection, scheduling, schema IDs/ordinals, units and validity policy.
- Preserve the existing 0.5 Hz procLog publication and 20-cycle smaps refresh. Caches identify processes by PID and start ticks; do not carry metadata across PID reuse.
- Rust-owned code must not invoke a Python interpreter at runtime. Native msgq remains an explicitly inventoried FFI dependency, not a completed native-service port.
- All data fixtures are synthetic. Actual host/device process names, command lines and identities stay in ignored local evidence.
- The user will apply dev to C3X later and supply feedback. Do not connect, install, restart or test on their device.
- The first unavoidable target gate is C3X ABI/IPC execution. Prepare checksummed static aarch64 diagnostic artifacts and a bounded probe; do not call an unrun probe a passed device test.
- No claim of complete runtime conversion or CPU/thermal improvement. #1 and device-dependent #5 remain open until their acceptance criteria are met.

## Review focus

1. A reused PID must invalidate both metadata and smaps caches even when its command name is unchanged.
2. A slow or disconnected subscriber cannot create an unbounded Rust queue or retain borrowed native memory.
3. Params writers must interoperate with the original .lock/rename/fsync protocol, including arbitrary binary values and clear flags.
4. An IPC probe must refuse an empty/production namespace and leave the existing Python publisher untouched.
5. Generic aarch64 build success is insufficient: inspect the candidate ELF architecture/dependencies, run emulated smoke checks where available, and leave actual C3X execution pending.

### Task 1: Complete procfs collection

Files: runtime-core/src/procfs.rs; runtime-core/tests/procfs.rs; runtime-core/src/lib.rs; Cargo manifests/lock.

Interfaces: `Collector::new(root: &Path, ticks: NonZeroU64, page_size: NonZeroU64)`, `Collector::snapshot() -> io::Result<Snapshot>`; `Snapshot` contains typed CPU counters, memory and process records. ProcessStat remains the shared stat parser.

- [ ] Write synthetic proc-tree tests: CPU/memory units, parentheses and signed counters, missing/malformed records, binary cmdline replacement, exact smaps cadence, PID reuse and exited-process eviction.
- [ ] RED: `cargo test -p openpilot-runtime-core --test procfs` must fail for the absent collector.
- [ ] Implement checked collection, cache identity and bounded file reads without publication or hidden Python.
- [ ] GREEN: run runtime-core suite; verify a real local snapshot only as host evidence.
- [ ] Commit the independently tested collector.

### Task 2: Canonical cereal schema and reference parity

Files: rust/crates/cereal/{Cargo.toml,build.rs,src/lib.rs}; rust/crates/proclogd/{Cargo.toml,src/lib.rs,src/wire.rs}; rust/tools/check_proclog_reference.py; tests and synthetic fixtures.

Interfaces: `encode_snapshot(snapshot: &Snapshot, log_mono_time: u64) -> Result<Vec<u8>, Error>` produces the existing Event.procLog wire message, valid=true. Generate log/car/custom/deprecated schemas from their existing repository files; no reduced or hand-copied schema.

- [ ] Write tests asserting decoded Event/ProcLog fields, signed values, float rounding, invalid conversion and exact caller timestamp.
- [ ] RED: run the wire tests before implementing encoding.
- [ ] Generate full bindings and implement conversion with checked integer and explicit float conversion.
- [ ] Compare every field against the actual source build_proc_log_message and helpers executed on the same synthetic proc tree using real pycapnp messages; include successive cache cycles and documented PID-reuse correction.
- [ ] GREEN: Rust wire tests and Python reference runner pass. Commit.

### Task 3: Real msgq ownership boundary

Files: rust/crates/msgq/{Cargo.toml,build.rs,src/lib.rs,src/bridge.rs,native/bridge.h,native/bridge.cc}; tests and native peer.

Interfaces: `Publisher::new(endpoint: &str) -> Result<Publisher, Error>`, `send(&mut self, bytes: &[u8]) -> Result<(), Error>`; `Subscriber::new(endpoint: &str, conflate: bool)`, `receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, Error>`. CXX UniquePtr owns every native object; no Send/Sync promise or borrowed receive pointer escapes.

- [ ] Write real transport tests for payload identity, empty timeout, malformed endpoint rejection, slow consumer/conflation, publisher reconnect and object cleanup.
- [ ] RED: run tests against missing bridge.
- [ ] Compile existing msgq sources behind small CXX adapters with exception-to-Result conversion and bounded sizes/timeouts.
- [ ] GREEN: native C++ peer and Rust communicate in both directions in a unique test namespace. Run appropriate address/undefined sanitizers for the FFI boundary; distinguish these from Miri, which cannot execute native msgq.
- [ ] Commit transport and evidence.

### Task 4: Compatible Params storage

Files: rust/crates/params/{Cargo.toml,build.rs,src/lib.rs}; generator; raw-storage integration tests and native reference probe.

Interfaces: `Params::open(root: &Path, prefix: &str) -> Result<Params, Error>`; checked key lookup, `get(key) -> Result<Option<Vec<u8>>, Error>`, `put(key, &[u8])`, `remove(key)`, `clear(flags)`, metadata/default access. Use existing key definitions as the generated registry; fail on unrecognized definition syntax instead of silently skipping keys.

- [ ] Write tests for binary/empty/missing values, unknown/path-like keys, native namespace symlinks, concurrent atomic writers, clear flag masks, unknown-file cleanup and default metadata.
- [ ] RED: run params integration tests before implementation.
- [ ] Implement the original disk protocol with atomic temp-file publication, .lock, file/directory fsync and RAII cleanup.
- [ ] GREEN: compare registry and file effects with native C++ Params in temporary stores; no actual vehicle Params access.
- [ ] Commit storage boundary and tests.

### Task 5: Standalone producer, CI and device handoff

Files: proclogd/src/main.rs; rust/tools device-probe/build helpers; .github/workflows/rust.yml; rust/port-status.json; rust/README.md; docs/rust-port/m1-validation.md; docs/rust-port/c3x-probe.md.

Interfaces: an explicit bounded one-shot/file mode and isolated-namespace publish mode. Default cadence 2000 ms; a probe captures real Event.procLog messages with the canonical Python consumer and writes local results. No production manager selection change in this step.

- [ ] Write CLI/process tests for help, invalid flags, empty namespace refusal, bounded frame count, serialized output, broken pipe and native consumer interoperability.
- [ ] RED, implement, then GREEN through the actual executable and reference consumers.
- [ ] Add compiler/oracle dependencies to host CI, retain all existing required gates, build normal aarch64 plus a checksummed static probe candidate. Verify ELF and run emulated smoke checks separately from real-device evidence.
- [ ] Document exact host coverage, native dependencies, device steps/results to return, and unchanged production selection. Update inventory honestly.
- [ ] Independent whole-branch review; fix demonstrated important findings with RED/GREEN tests.
- [ ] Push; inspect exact-head fast/Rust/docs/integration results; merge through PR; inspect separate merge-SHA results.
- [ ] Stop at the user's C3X execution boundary with the exact dev commit, candidate SHA256 and test instructions. Retain #5 open for device acceptance and #1/#6 for broader work.

## Self-review and execution decisions

Tasks 1 -> 2 -> 5 share Snapshot; 3 -> 5 shares byte transport; 4 is independently exercised by the device probe. No task publishes to the production procLog namespace. The full runtime design remains unchanged; target execution is an explicit intermediate gate before replacing production processes. Native execution and one final review preserve the user's existing preference and autonomous authorization.

Tasks 1-4 have passed their host tests and source/native reference comparisons.
Task 5's executable and static candidate have passed host and emulated checks;
independent review, exact-SHA CI, dev merge and post-merge checks remain before
handoff. See m1-validation.md for the actual evidence and limitations.
Under the repository's explicit user-documentation policy, the target probe is
documented as a developer handoff here; this task does not change vehicle settings
or edit the public Korean/English user guides.
