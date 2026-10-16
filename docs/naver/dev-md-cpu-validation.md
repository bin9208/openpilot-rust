# dev-md CPU saturation investigation — #344

Tracking: [#344](https://github.com/bin9208/openpilot-rust/issues/344), related
[#338](https://github.com/bin9208/openpilot-rust/issues/338). Work remains on
`dev-md`; device installation and driving acceptance belong to the user.
The parent performs source research, design and final review; the user-selected
`llama-cpp/Qwen3.8-27B-UD-Q6_K_L.gguf`, explicitly configured with `xhigh`,
authors code patches. The parent reviews and integrates those artifacts.

## Recorded baseline

The supplied private route was decoded with the full `openpilot.cereal.log.Event`
schema, not the reduced CAN schema. Its clean source is
`c78a18639efdd817ebd78970123d13907479f7a8`, branch `dev-md`, AGNOS
`19.8-carrot-bt1`, device family `tizi`. Raw logs and identifiers stay outside Git.

Thirty procLog samples span 58.0974268 seconds. Process CPU is the difference in
user plus system seconds divided by elapsed monotonic time, matching PID and
start time. Percentages use one logical core as 100%; multithreaded processes
are not clamped. The 106 deviceState observations report these core means:

| Core | Mean CPU % |
| --- | ---: |
| 0 | 99.20 |
| 1 | 99.00 |
| 2 | 99.65 |
| 3 | 98.10 |
| 4 | 99.94 |
| 5 | 99.94 |
| 6 | 60.79 |
| 7 | 9.59 |

| Process | CPU % | User % | System % |
| --- | ---: | ---: | ---: |
| encoderd | 303.68 | 296.45 | 7.23 |
| loggerd | 66.91 | 34.17 | 32.74 |
| pandad | 45.08 | 6.78 | 38.30 |
| camerad | 34.58 | 27.01 | 7.57 |
| ui | 16.85 | 13.37 | 3.48 |
| card | 16.71 | 10.29 | 6.42 |
| driving-modeld | 14.80 | 10.28 | 4.53 |
| locationd | 14.41 | 12.31 | 2.10 |

All 503 road, 733 driver and 486 wide recording indices have type
`bigBoxLossless`; 728 qroad indices have type `qcameraH264`. There are 880
encoder buffer-lag reports (362 road, 286 driver, 232 wide). All three camera
services have 1,197 messages with consecutive frame IDs in this segment.
Publication timestamps alone do not establish physical sensor timing.

## Cause and bounded design

The actual candidate recipe enables `openpilot-encoderd/native`, while camera
and model packages enable `openpilot-msgq/visionipc-ion` transitively. The
encoder tests its own `cfg!(feature = "visionipc-ion")`, which remains false.
Its transport therefore uses ION while codec and platform selection use the
PC policy: FFVHUFF lossless software encoding instead of the intended V4L2
HEVC. The logged codec indices independently confirm this path.

Read-only Cargo resolution reproduces the mismatch with the affected packages:

```sh
cargo tree --offline --locked --manifest-path rust/Cargo.toml \
  -p openpilot-encoderd -p openpilot-camerad-runtime \
  --features openpilot-encoderd/native,openpilot-camerad-runtime/visionipc-ion \
  -f '{p} features={f}'
```

The encoder resolves to `default,native`; its shared msgq resolves to
`default,native-skip-miri,visionipc-ion`. The pinned original `encoderd.cc`
selects V4LEncoder under `__TICI__`. Device main/stream encoders retain priority
52 on core 3, and optional Carrot Vision retains normal scheduling on core 0.

This follows Cargo's documented feature behavior: features belong to their
defining package, while dependency features are unified across consumers.
See the [Cargo Book](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification).
The repair shares the compiled transport fact with the encoder and checks
ordinary PC, dependency-only ION and explicit encoder ION configurations.
Hardware codec tests must not instantiate vehicle devices on the host.

The unfinished camera sampling optimization is reviewed separately: advance
sample offsets incrementally and obtain exact count/last offsets without
repeating division and remainder for every byte. Preserve the source AE
indices and luminance bits, including padded images, exhausted iterators and
large legal sampling intervals. Shared-memory sample reads retain bounds
checks on every actual offset and the existing atomic-access contract.

No codec quality, frame rate, control/model cadence, validity threshold or
safety behavior is reduced. Restore the intended device scheduling policy;
do not invent a new CPU placement. Logger load may decrease with smaller
encoded payloads, but its separate improvement is not measured here. The log
also contains Athena restarts; existing unfinished Athena changes remain
outside this CPU patch.

## Validation and delivery status

Focused host verification on 2026-10-17 used the coordinated Cargo cache,
incremental compilation disabled, two jobs, and dev/test debug information
disabled. Disk checks remained about 222 GiB free, above the required 25 GiB
plus estimated growth. Existing native SDK, FFmpeg and libyuv were reused.

- Encoder RED extracted the original local-feature selector into the helper
  used by production. Ordinary PC passed; dependency-only ION failed by
  assertion: `BigBoxLossless` versus required `FullHevc`. After switching the
  helper to the shared transport fact, all three configurations passed their
  three unit tests: `--features native`,
  `--features native,openpilot-msgq/visionipc-ion`, and
  `--features visionipc-ion`, using
  `cargo test --manifest-path rust/Cargo.toml -p openpilot-encoderd --lib --locked`.
  Tests verify codec and main/stream/optional scheduling policy without opening
  hardware or making scheduling syscalls.
- Sampling RED reproduced an overflow after the final valid sample with
  `sampling.width = usize::MAX / 2 + 1`, two one-pixel rows and conceptual
  `pixel_length = usize::MAX`. No image allocation is needed. The final-sample
  guard makes all three geometry tests pass, including existing exhaustive
  offset/luminance equality and new partial/exhausted count/last checks.
- The affected host package run passed:
  `cargo test --manifest-path rust/Cargo.toml -p openpilot-camerad -p openpilot-msgq -p openpilot-camerad-runtime -p openpilot-encoderd --features openpilot-camerad-runtime/native-skip-miri,openpilot-encoderd/native --all-targets --locked`.
  This includes native VisionIPC ownership/frame-state checks.
- The additional memory test covers a real one-byte pixel tail, valid
  nonmonotone caller order, an invalid middle index followed by a valid last
  index, and unchanged frame-ID storage. It does not prescribe partially
  written destination contents after errors. The msgq library's 40 unit tests
  passed after adding it.
- Strict all-target Clippy passed for the same four packages/features with
  `-- -D warnings`. Formatting and `git diff --check` passed.
- The existing encoder CI isolation-policy test passed. CI now executes both
  dependency-only and explicit ION policy configurations, with disk guards,
  alongside its existing PC/source-oracle checks.

Parent review checked that every actual sample read still validates its offset
before the atomic accessor, that incremental additions only advance to a real
next sample, and that video creation, logging identity and scheduling all use
the same backend helper. Existing Athena/Cargo changes are preserved separately.

The unchanged-payload preflight checked 995 existing manifest entries totaling
1,272,538,813 bytes with no hash mismatches; 87 project runtime executables are
rebuilt for the new source before delivery. Exact final SHA, Actions URLs,
sealed AGNOS prerelease and downloaded-asset verification are recorded in #344
after source freeze. Source tests, release readiness and user device acceptance
remain separate. No device CPU reduction, startup or recording/upload result
is inferred from host tests.
