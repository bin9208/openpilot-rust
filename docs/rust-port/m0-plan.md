# Rust foundation implementation plan

Goal: establish testable scalar filter ports and read-only Linux process CPU measurement in the independent repository. This is M0 of the approved full runtime design, not completion of #1.

Spec: `docs/rust-port/design.md`. Source: `f3a92524d87be714f6b8b5f44ecdc8319a8c53d1`.
Issues: #2 (implementation), #3 (CI). Execution: native implementation followed by independent review. User authorized proceeding autonomously in the new repository.

## Global constraints

- Never modify or push to bin9208/openpilot or its D:\Openpilot checkout.
- Keep production daemon selection and all vehicle behavior unchanged in M0.
- External native libraries may remain behind FFI; no hidden Python runtime may be counted as a complete Rust conversion.
- Distinguish unit tests, Linux execution, aarch64 build, AGNOS execution, vehicle acceptance and performance evidence.
- Rust percentages are per logical core; no >100% clamp. PID plus start time identifies a process.
- No private route logs or machine identifiers in committed test fixtures.
- Pin Rust 1.94.0 and use no runtime third-party crates in this first workspace.

## Task 1: first scalar filter ports and CPU parser

Files: `rust/Cargo.toml`, `rust/rust-toolchain.toml`, `rust/crates/runtime-core/src/{lib,filters,proc_stat}.rs`, `rust/crates/runtime-core/tests/{filters,proc_stat}.rs`.
Interfaces: FirstOrderFilter::new/update/update_alpha/value; BounceFilter::new/update/value; ProcessStat::parse and cpu_percent(previous,current,elapsed,ticks_per_second).

- [ ] Write tests first: first-order step response, cold initialization, changing RC, zero RC, bounce threshold/long sequence; stat names with whitespace/parentheses, truncated/bad numbers, PID reuse, reset counters, elapsed=0, ticks=0, >100% multithread values.
- [ ] Run cargo test and retain expected missing implementation failure.
- [ ] Implement source-compatible scalar filters (finite valid timing domain) and checked integer /proc counters.
- [ ] Run fmt, clippy and test; no production integration.

## Task 2: differential oracle and real Linux sampler

Files: `rust/crates/runtime-core/examples/filter_trace.rs`, `rust/tools/check_reference.py`, `rust/crates/runtime-core/src/bin/cpu-sample.rs`, `rust/crates/runtime-core/tests/sampler.rs`.
Consumes Task 1 API. Produces deterministic TSV filter traces and a one-shot, opt-in TSV Linux CPU sampler.

- [ ] Test sampler invalid CLI intervals and run against Linux /proc. Sampling 1..60000ms; clocks use Instant, ticks from checked getconf CLK_TCK.
- [ ] Load the actual baseline Python filter classes via importlib, compare deterministic step/ramp/random/RC-change sequences at absolute+relative 1e-12 tolerance. Python is a test dependency only.
- [ ] Verify multi-thread percentages, raced-away processes, empty/unreadable records and PID reuse do not crash or fabricate results.
- [ ] Record host sampling separately from target evidence.

## Task 3: independent CI and port status

Files: `.github/workflows/rust.yml`, `rust/port-status.json`, `docs/user/{ko,en}/rust-port.md`, `docs/rust-port/validation.md`, repository-local AGENTS addendum.

- [ ] Rust fmt/clippy/tests/reference on all branch pushes and dev/main PRs, Linux x86_64 release build.
- [ ] aarch64 Linux GNU release build on dev/main PRs and post-merge pushes, artifact retained; no claim of AGNOS ABI or runtime validation.
- [ ] Keep inherited required gates and verify real latest-SHA runs. Disable inherited publishing/sync workflows after reviewed safe registration.
- [ ] Validate mapped docs and branch protection readback. Create PR and attach it to the chat.
- [ ] Independent review, fixes, final latest-revision checks; merge only after all required gates and review pass.

## Review focus

1. Parser names can contain ')' and whitespace: split on the final ')' and keep integer counters until differencing.
2. PID reuse/counter resets must yield no sample, not underflow or a false spike.
3. Cold filters and rc changes must match the actual Python class at every step, not just steady-state output.
4. Linux build success does not prove compatibility with the device's older glibc/kernel or a hardware driver.
5. CLI failures return nonzero and useful diagnostics; production runtime is not switched on by merging M0.
