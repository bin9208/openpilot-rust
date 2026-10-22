# Explicit exact-source runtime request — #486

The user requested publication after `git pull` and reboot left the updated
device unable to start. `launch_openpilot.sh` invokes `ensure_runtime.sh`, which
requires a source-matched installed manifest or downloads the release for the
actual Git HEAD. The unmarked CI merge has no corresponding release yet.
Device logs are unavailable; this records the verified source/release gap,
not a claim that other device faults have been excluded.

- Request: https://github.com/bin9208/openpilot-rust/issues/486
- Runtime source: `dc95138076836ea3a5c1b6c27262be3f3b912f71`
- Required push CI: https://github.com/bin9208/openpilot-rust/actions/runs/39699486761
- Existing successful PR CI: https://github.com/bin9208/openpilot-rust/actions/runs/39603474222
- Controller branch: `codex/fix-486-exact-runtime-delivery`

The original automatic marker-only workflow remains unchanged on dev-md. This
issue branch adds a manual dispatch input referencing an open owner-authored
issue containing the exact source SHA, push CI run, controller ref and explicit
publication authorization. Owner identity, actual merged PR, current dev-md
head, completed latest push CI and all five successful exact-source gates are
checked before any package build. An incomplete CI run is waited for, never
accepted as success. Pure approval/evidence tests run in the normal fast CI.

The immutable requested source is checked out first, preserving its existing
temporary storage and Rust PATH initialization. The controller is retrieved at
the exact dispatch commit into owned runner temporary storage. The requested source's
`package.py` is loaded as a library, without modifying its marker-only entry
point or any runtime source. Its own source policy, baseline/sysroot/provider
verification, complete AGNOS build, sealing and offline installer checks are
used. Publication verifies the exact-source publisher and installer hashes;
that native publisher independently verifies latest exact-SHA CI and all
sealed assets before publication. The source's online installer verification
then checks the public release. Controller approval receipts accompany the
Actions evidence artifacts, separate from the six runtime release assets.

Local validation is limited to syntax, formatting and diff review. Functional
tests and all builds/install verification run in GitHub-hosted Actions. The
runtime source and protected branch are not changed by the controller; device
connection, installation and reboot remain user-owned. Publication and online
verification are pending; append exact Actions/release evidence to #486.
