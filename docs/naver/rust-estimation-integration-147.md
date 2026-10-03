# Native estimator integration (#147)

Tracks [#147](https://github.com/bin9208/openpilot-rust/issues/147) under
[full runtime #1](https://github.com/bin9208/openpilot-rust/issues/1).
The integration follows GNSS #137 and combines locationd #138 at `46570176`,
lagd #139 at `d015e48a` and paramsd #143 at `bb7bbb74`.

Rust owns all three daemon policies, their project-specific equations, cache
handling and publication decisions. The unchanged external rednose/Eigen solver
and pinned PocketFFT kernel remain explicit native dependencies. Python and the
original generated model libraries are comparison oracles only.

## Integration and required checks

The workspace and candidate catalog include all three processes, without changing
production selection. The Cargo resolver adds the new workspace packages while
retaining existing external versions. Eigen is installed from the source lock's
`commaai/dependencies` commit `40e5d76de1b33a86c5181b63db6782d8f06da1da` in
workspace, estimation and ARM jobs. Both numerical adapters use that same header
directory.

The new `rust estimation runtime` job is required by `rust checks`. It builds
the actual source oracles and native binaries, compares the full estimators and
main loops, exercises cache migration, private IPC, persistence/restart and
shutdown, and instruments the production numerical ownership boundaries.
Source and native artifacts are retained even when the job fails. Existing
checks and the generic GNU/musl ARM workspace builds remain required.

Component contracts and reproducible commands:

- [Location model, loop and native boundary](../rust-port/locationd-validation.md)
- [Lateral delay numerical and daemon contract](rust-lagd-139.md)
- [Vehicle model, cache and native process](../rust-port/paramsd-validation.md)

## Location cache startup regression found during integration

The source Cython Params getter maps an empty file or a filesystem read error to
`None`. The first native locationd candidate instead passed empty bytes into
cereal, or propagated the read error. The independent startup comparison
reproduced native exit 1 for empty and directory-valued
`LocationFilterInitialState`, while the unchanged source initialization accepted
both as absent. Missing values already worked, and corrupt nonempty values failed
in both implementations.

The native startup boundary now applies the same empty/read-error conversion.
It still rejects malformed nonempty cereal and propagates non-I/O Params errors.
`rust/tools/check_locationd_startup.py` executes the original initialization block
with the real original Params binding and compares all four cases to a native
process. The native process receives SIGTERM only after its private publisher
exists, so signal registration precedes termination.

Local RED/GREEN, package tests and continuous IPC artifacts are retained under
`.analysis/scratch/2026-10-01-rust-estimation-integration/`. After the fix, the four
startup cases pass and the native continuous/seeded cases publish 115/8 messages
equal to the source, with SIGINT/SIGTERM exit 0. Existing location package tests
pass. The new regression is also required in the estimation CI job.

## Limits

Exact-SHA Actions and separate post-merge status must be recorded before component
integration is closed. Host source parity, sanitizers and generic cross-builds do
not establish AGNOS execution, physical sensor behavior, vehicle timing or CPU
savings. The complete runtime, normal startup and existing log-upload path remain
open under #1; the user performs the first device comparison afterwards.

Docs-Not-Needed: implementation-language conversion and source-compatible startup
repair; no public setting, guide behavior or production selection change.
