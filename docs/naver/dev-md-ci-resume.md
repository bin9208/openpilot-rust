# dev-md CI resume: exact 7bc9c45e failures (#338)

Investigated [run 38681313689](https://github.com/bin9208/openpilot-rust/actions/runs/38681313689), head `7bc9c45ef710d28c546dda249c4badac33c08f6c`, through exact per-job log API (no large artifact archive). All five assigned failures reproduce deterministic schema/reference-snapshot mismatch; none is classified as flakiness. No runtime thresholds, comparison tolerances or scheduling acceptance windows are relaxed. No device or protected-branch operation is involved.

| Failed job | Root cause and repair | Tracking |
| --- | --- | --- |
| [sensor/audio 116246281591](https://github.com/bin9208/openpilot-rust/actions/runs/38681313689/job/116246281591) | `check_soundd.py` case0 step110 expects alert0/frame0 from stale retained source, but native handles newly imported SystemReady37/frame2. Source tip `e8e152db` maps SystemReady to existing prompt.wav, one play/MAX_VOLUME. Import that exact source mapping only; retained sound source remains a mixed snapshot. Preserve full sample/state comparison. | [#340](https://github.com/bin9208/openpilot-rust/issues/340) |
| [selfdrived 116246281771](https://github.com/bin9208/openpilot-rust/actions/runs/38681313689/job/116246281771), [workspace 116246281876](https://github.com/bin9208/openpilot-rust/actions/runs/38681313689/job/116246281876) | `messaging/tests/state.rs` incorrectly expects customReservedRawData1 to be missing after additive Event@154 import. Move it into the existing all-service successful payload/default validation path. Unknown navModel/customReservedRawData2 still require FieldNotFound. No messaging production code changed. | [#341](https://github.com/bin9208/openpilot-rust/issues/341) |
| [radard ARM 116246281515](https://github.com/bin9208/openpilot-rust/actions/runs/38681313689/job/116246281515), [radard host 116246282161](https://github.com/bin9208/openpilot-rust/actions/runs/38681313689/job/116246282161) | Both report old fixture byte hash9b5242… versus832616…. Additive schema struct sizes change serialized bytes while packet values remain equal. Retain legacy bytes and hash plus compare every decoded field/default before using new layout fingerprint. | [#342](https://github.com/bin9208/openpilot-rust/issues/342) |

## Radard fingerprint preservation evidence

Generated the unchanged packet constructor's 40 frames ×4 topics under isolated `capnp.SchemaParser` with all schemas from pre-import `b55552db`. Their concatenated bytes reproduce old exact SHA-256 `9b52421501fc4cd5487623d1b61c5489067d2e6e6f1862e4fe82c78df53620ad`. Kept all160 messages in a 2528-byte deterministic gzip/base64 fixture `rust/tools/tests/fixtures/radard-legacy-160.json.gz` (synthetic inputs only).

New regression verifies legacy count/hash, decodes every legacy packet with current schema, and requires equality of the complete `to_dict()` to the current packet constructor. All160 comparisons pass including newly introduced default identities. The current byte fingerprint is separately pinned as `83261653c789aa715d24f56edba21e42bdc2bc252fbb1d20b5ac056833e204a9`. Thus a change to any packet field or timestamp fails semantic/byte checks; a schema allocation change fails current byte fingerprint but cannot silently replace legacy values. Native-IPC-unavailable subprocess retains the same exact new-layout fingerprint check. Original timestamp anomalies, 20-frame warmup and25ms availability margins remain unchanged.

## Local validation

* RED messaging: exact `cargo test --manifest-path rust/Cargo.toml -p openpilot-messaging --test state` reproduces named failure (4 pass/1 fail).
* GREEN same messaging target: all5 pass after expectation repair.
* RED radard: existing unittest reproduces exact old/new fingerprint mismatch and isolated-import subprocess error.
* GREEN `PYTHONPATH=rust/tools:.:opendbc_repo .venv/bin/python -m unittest discover -s rust/tools/tests -p test_radard_ipc_preconditioning.py -q`: all6 pass, including new complete legacy semantic equivalence test.
* `cargo test --manifest-path rust/Cargo.toml -p openpilot-soundd`: all3 playback regressions pass, including real prompt asset equality and volume policy. Full original-source comparison also passed after building original Params below.
* Scoped diff/format checks performed. Builds use coordinated `/home/fma1/rust/cache/cargo-target`, incremental0, debug0, two jobs and native-sdk bin PATH; disk365GiB available before each build.

The run remained in progress when logs were inspected; its aggregator cannot be treated as an independent defect or acceptance evidence. Exact revised-head cloud results and downstream selfdrived/workspace gates remain outstanding until rerun. Issues remain open for those acceptance gates. This record does not claim complete latest-source or device parity.

## Full sound source rerun

Built the unchanged original Params Cython/C++ binding with the existing `.venv`, locked hash-verified json11 dependency and native SDK headers; disk guard365GiB before download/build. No daemon or audio device was opened. Invoked `check_soundd.py` with shared-cache `debug/examples/sound_trace`, the freshly built binding and SDK libzmq library path. All18 binding source hashes verified. Result: **passed, six scenarios ×141 steps, 6348 sample comparisons, audio_initialized=false**. Scalar tolerances remain1e-12; samples retain exact comparisons. Initial import without SDK LD_LIBRARY_PATH failed to locate libzmq.so.5, corrected by supplying the existing SDK library directory; no source or dependency workaround was introduced.

## Previous committed-head ARM completion

Run 38681313689 on 7bc9c45ef710d28c546dda249c4badac33c08f6c is now complete.
Its rust aarch64 build job passed; aggregate conclusion remains failure due to
the recorded component failures. This is evidence for that exact historical
head, not for the continuing uncommitted dev-md implementation. Revised-head
complete CI remains required after the remaining port/integration work.
