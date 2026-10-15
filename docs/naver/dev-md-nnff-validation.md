# dev-md bounded NNFF startup-notice port

Track: [#338](https://github.com/bin9208/openpilot-rust/issues/338), experimental `dev-md` only. The stable `dev` runtime work remains separately tracked in #296. No branch promotion, commit, push, device operation or runtime selection change was performed by this subtask.

Source behavior: `1f26523cf5dac331213c262fd81ee058f6492faa`, inspected at final source pin `e8e152dbcfd9574a1791ed12026d1880e87689b3`. The NNFF portion is implemented; the same source commit's **search-only NNFF setting/UI and related guide/catalog work remains pending**, so this is not completion of the entire commit or all upstream work.

## Behavior

The prior Rust notice was emitted only at frame550, before alert selection. The native controller now polls at frame550 or later, on frame multiples of50 (2Hz at the existing100Hz control rate), after the normal alert selection. It requires initialization, nonpassive operation, started device, valid/non-timeout CAN, healthy subscription checks, no NO_ENTRY event, and no selected alert. It reads NNFFModelName only after these gates, including a late write. An absent model remains retryable; a present model adds the existing TorqueNNLoad event and reruns selection in that frame. The one-shot flag is set only after successful selection; read/selection errors retain retry eligibility.

No new alert types or readiness policy were added. The source's half-second system-ready chime is a separate feature; this NNFF slice preserves the Rust controller's existing readiness behavior. Future integration of that feature must call it before NNFF, matching final-source ordering. NNFF does not introduce separate replay/simulation exclusions.

Changed native targets: `rust/crates/selfdrived/src/controller/motion_events.rs`, `controller/mod.rs` (one-shot state and post-alert hook), `controller/initialization.rs` (false initial state), and `controller/motion_events/tests.rs`.

## Test-only original-source reference

The retained `openpilot/selfdrive/selfdrived/selfdrived.py` is deliberately a mixed snapshot: its pre-existing baseline remains, with only the exact NNFF initialization field, removal of the old frame550 branch, final-source `update_nnff_alert` method, post-alert invocation from1f26523c, and the required existing EmptyAlert import applied. It is not represented as the full latest source file. This prevents existing native/original-source comparisons from silently comparing different NNFF delivery policies. It does not enable Python in the native runtime.

`rust/tools/tests/test_nnff_reference.py` executes the actual reference method AST without importing device/native modules. Its initial RED run executed the old inline branch and failed for late readiness and seven independent eligibility guards; after the minimal reference patch both tests passed. The harness uses synthetic dependencies only at the Params/message/event boundary; the native tests additionally exercise real Rust Events and AlertManager selection.

## Validation evidence

- Before behavioral implementation, the original branch was mechanically extracted into a private NNFF method; six controller regression tests all **failed** on expected behavior assertions. Missing readiness guards, failed late retry, ignored alert-selection errors and stealing an existing alert were observed. The method extraction itself retained old semantics.
- After implementation, `cargo test --manifest-path rust/Cargo.toml -p openpilot-selfdrived --lib nnff -- --nocapture`: **6 passed**.
- `cargo test --manifest-path rust/Cargo.toml -p openpilot-selfdrived`: **7 library tests +9 integration tests passed**; binary and doc targets passed with no tests.
- `cargo clippy --manifest-path rust/Cargo.toml -p openpilot-selfdrived --all-targets --no-deps -- -D warnings`: **passed**, no warnings.
- `python3 -m unittest discover -s rust/tools/tests -p test_nnff_reference.py`: **2 passed** after reference update.
- `rustfmt` on the changed Rust source/tests and scoped `git diff --check`: passed.

All Cargo work used the coordinated `/home/fma1/rust/cache/cargo-target` (the checkout's `rust/target` is a symlink to that path), incremental disabled, two jobs, dev/test debug0. Disk guard checked before each operation:366GiB available. Initial environment attempt failed because assembler was absent from PATH; the working native SDK suffix is `/home/fma1/rust/cache/native-sdk/usr/bin`. An initial RED attempt was also blocked by the newly synchronized schema's `AudibleAlert::SystemReady` exhaustive match in beepd; the model/sound owner fixed that separately before the observed six failing behavior tests. A transient struct-literal parsing typo during implementation was corrected before GREEN; no unresolved failures remain in the above commands.

Coverage includes late initialization, late model availability, absent model, 2Hz reads, passive/offroad/CAN timeout/invalid CAN/unhealthy subscriptions, existing warning and NO_ENTRY deferral, real selected torqueNNLoad alert, one-shot suppression, Params read error and alert-selection error retry. Full workspace/source-reference equivalence, startup-chime integration, CI exact-SHA acceptance, target hardware and first-drive evidence remain parent integration work. This evidence does not establish complete runtime parity or CPU savings.
