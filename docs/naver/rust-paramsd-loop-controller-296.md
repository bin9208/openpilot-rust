# Native paramsd loop controller preparation (#296)

`check-paramsd-loop-native --binary ... --evidence ...` constructs the complete
39-case input in Rust and first requires exact equality with an independently
captured original input. It runs the current host ELF through the existing
owned-process runner, with cancellation,180-second child timeout, concurrent
bounded stdout/stderr capture, and a32MB raw-output limit before hashing and
comparison. Existing evidence is rejected and never overwritten.

The expected initial states and all2,420 rows come only from the unchanged
original `loop/source.json` in run39514415599/job118943760823,
artifact12306774260. The parent verified both compressed fixtures decompress
to the exact original-only artifact members, not its native result. See
`rust/crates/dev-tools/tests/fixtures/paramsd-loop-original/README.md` for
raw/stored hashes and the one precisely located negative-infinity observation.

The comparator checks all initial values/covariances, ordered operations,
error-log normalization, decoded Cap'n Proto Events, state arrays, cache and
GPS decisions. The original scalar tolerances are unchanged. Coverage is
39cases,2,420frames,4saves,216GPS writes. Before writing its exclusive result,
the controller verifies input, executable, source and output identities and
cancellation again; interrupted or failed stdout publication does not create
a success receipt. Signal handlers remain registered during finalization.

The mandatory cloud step retains the existing fresh Python original checker,
executes the native CLI with unavailable PATH, checks its exact checker and
candidate exec vectors, compares its raw output independently, and runs
negative tests plus strict Clippy. New tests reject changed input metadata,
packet/order changes, wrong source aliases, scripts instead of native ELF,
changed artifacts, missing/invalid/oversized output, child failure, cancellation
and foreign evidence. The source comparator and controller have been reviewed;
actual cloud execution at source `60e3711e09d6b45b66a3f5ac30d79d6521f86e0c`
passed in [run39523577819/job118972931583](https://github.com/bin9208/openpilot-rust/actions/runs/39523577819/job/118972931583).
The actual CLI compared39cases/2,420rows/4saves/216GPS writes;85library tests,
four controller regressions, two independent reference/exec tests, strict Clippy,
and13disk/68source/2lagd policies passed. Artifact12311849238 is92,992bytes,
archive SHA256 `ca02b858158d14785bc41ab68f45a2421bcec30863838b8ea7b467c8e9d7f616`.
Parent inspection confirmed the actual raw/request case/row counts, all eight
unchanged source/input/ELF bindings plus the new output binding, exact retained
input/output hashes, and two successful native execs. The workflow commit
`2bd29771ae30832433a4cb7c7adcfc876bb4d354` is separate from that tested source.
Neither the Python loop checker nor its fixture/source helpers is retired yet.
