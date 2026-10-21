# Strict UI FPS diagnosis (#384)

The full source8e8b1e8f Rust UI/connectivity job [118974708157 in run39524132398](https://github.com/bin9208/openpilot-rust/actions/runs/39524132398/job/118974708157) failed the application reference's `strict` case: retained expected exit0, observed1, diagnostic `application render: Failure("strict application exit")`. The PNG exists. Artifact12315022858 ZIP SHA256 `c48ae0168c4ffb67c94d1bb59759eed14ad9aaa5aa8a9ecbd2fdbcc4701b6f87`. This continues [open issue384](https://github.com/bin9208/openpilot-rust/issues/384); it is separate from the earlier UI buffered-frame bug.

Both original and native application monitor actual raylib FPS after rendered frames and exit below10 in strict mode. OFFSCREEN=1 sets an uncapped frame limit; PROFILE_RENDER=2 ends after two frames. The retained artifact lacks actual FPS samples, so this failure does not prove a native-only semantic defect or justify changing the expected exit/threshold.

The native reference now has opt-in `UI_APPLICATION_FPS_DIAGNOSTIC=1` in-memory sampling of real GetFPS values and elapsed frame time, emitted after rendering. Default behavior and the maintained full20case test remain unchanged; no environment-controlled case filter is added to that required test.

A diagnostic-only workflow on the isolated proof branch restores exact-hash original helper code to runner scratch, instruments only observation of real GetFPS, and runs original/native on one Xvfb display. Only a detached scratch checkout receives a recorded test-only strict-case filter. This is diagnostic historical execution, not retirement or a maintained Python dependency. Raw FPS, exits, frames, source/workflow identities and host load are required before choosing a repair. Focused reproduction may not reproduce full-job load; the full matrix remains required.

Status: source ready; diagnostic and final unchanged strict acceptance pending hosted Actions. Local checks are syntax, formatting and diff only. No device or driving claim.
