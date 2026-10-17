# dev-md final-source Panda boundary port (#338)

The subsequent native SPI wait optimization and its timing/CPU evidence are
tracked in [the per-core follow-up](dev-md-core-cpu-validation.md) under #348.

Source target `e8e152dbcfd9574a1791ed12026d1880e87689b3`, delta from `43203371004e035bdb70a00a8dad29a4b657c6c3`. Project-owned host orchestration remains Rust. Independent safety MCU C firmware stays an explicit native boundary; no flashing, device connection or protected-branch change is part of this port.

## Contracts being imported

* Health ABI17:59 bytes, wake byte inserted at34; every later legacy field shifts by1. Match source Python/C/host decoders and version checking together. Publish wake separately from ignition; wake alone never starts onroad.
* Tesla wake image selected only with TeslaWakeOnCAN and supported Model3/Y evidence; persistent/current CarParams take precedence over preselection. Invalid identity/bytes never authorize wake. Selected F4/H7 image name, signature and flashing use the same exact variant; missing selected image fails closed. Existing stock defaults remain. Supervisor verifies selected image before legacy child's stock-name-only check is bypassed, matching source handoff semantics.
* Exact-source MCU changes: checksum/rolling-counter validated Tesla power/gear/cabin evidence, invalid sequence reset and expiry, stock/wake build separation, rising wake bootkick, H7 narrowly gated live-controller preservation and bounded old-policy queue clearing, timer interrupt registration and sound register fixes. No safety predicate or threshold is broadened.
* Native Rust firmware builder/staging/package must produce and verify both signed wake variants in addition to stock images/bootstubs. Build-input admission/provenance must match the final SConscript rather than skip its hash check.

## Evidence so far

Imported final-source Panda tests first. Native RED found the old2argument bootkick API incompatible with the new wake test (after adding existing native SDK assembler to PATH). Applied exact source board and retained Python/C++ oracle deltas. Compiled owned host wake+stock libpanda fixtures with existing compiler/SDK, no hardware. All45 source ignition, stock-no-wake and bootkick tests pass. Four source firmware-selection tests also pass using the freshly compiled original Params binding and real capnp CarParams.

Rust RED: health59 packet test failed expected58 and missing wake field. Builder stage regression failed because its six-artifact fixture exceeded the old four-artifact verifier; original content-identity fixture also correctly detected changed source. Installer wake-package regression rejected the previously unallowlisted archive paths. All were repaired at their actual boundaries.

## Completed host/build evidence

* `cargo test -p openpilot-pandad`: all50 integration tests pass, including ABI17 offsets/bits, wake vs ignition wire/state semantics, exact MCU variant names/identity gate, missing image rejection and selected variant retained through actual DFU recovery/reconnect.
* `cargo test -p openpilot-dev-tools --test panda_build`: all8 pass (all-six immutable stage/signature/source binding, headers/version, CLI rejection, publication failure behavior).
* `cargo test -p openpilot-runtime-installer`:8 unit +46 CLI tests pass, including installing both new wake assets and verifying their sealed hashes. Archive path allowlists now admit both variants; parent owns corresponding manager-catalog required-file additions, which make complete packages require all6 firmware files.
* `cargo clippy -p openpilot-pandad -p openpilot-dev-tools --all-targets -- -D warnings`: pass. Scoped rustfmt/diff checks pass. Installer's later allowlist/test change passed its full suite; workspace clippy remains the parent integration gate.
* Native bins/examples and `build-runtime-panda` compiled. Actual pinned Arm GNU Toolchain13.2.rel1/13.2.1 build passed for all6 images. Content identity independently derived with original `panda/firmware_version.py`: `DEV-0914fbcd5f62-DEBUG`, agreeing with native implementation. Board/SConscript admission SHA256 is `eae1da08e51f33daa0c0846a5b59ae10f84751040234bbb6900cbfb7c68c9fa4`; no admission check was removed. Receipt reports source_unchanged=true and complete artifact hashes/sizes. No firmware was staged into an active runtime or flashed.
* All24 changed retained Panda/source tests under panda and pandad byte-match source tip e8e152db. Additionally imported exact `tools/tests/test_panda_safety_transition.py`: all7 production-function/mock-register tests pass for H7/F4 transition paths and bounded sleep acknowledgment. Combined firmware source tests:45 ignition/stock/bootkick +7 transition tests; firmware selector source tests:4 pass.
* Source/native client differential:2486 scenarios,45878 ordered calls PASS, including rejecting old v16 ABI. Source/native supervisor model:4183 scenarios,204933 calls PASS. Neither comparison was weakened.
* Compiled original C++ device/state oracles against current schemas and ABI17. Device:1749 scenarios/2493 operations PASS (all packed fields/bits, short/failing reads, control order). State:16 scenarios/4632 steps PASS with exact complete cereal bytes and ordered read/write/log/reconnect/heartbeat effects, including new independent wake field.
* Actual native supervisor/regular Params+firmware files/owned scripted USB ABI/live log IPC/native child:167 scenarios PASS. Checks exact transfers, logs, Params, child arguments/cwd/manager context and inherited descriptor closure. Initial manual invocations used the wrong owned USB fixture family and a relative evidence path; these safely aborted or failed child-trace checks. Correct CI-equivalent scripted ABI fixture and absolute evidence paths passed the full unchanged comparison.

All builds used disk guards (364–365GiB available), coordinated target cache, incremental0, debug0 and jobs2. Existing SDK, Python environment and locked compiler were reused. Local binary fixtures/build snapshots are ignored scratch; no binary artifact is committed.

## Deliberate native recovery improvement and limits

Upstream `pandad.py` flashes an explicitly selected wake path, but the lower-level Python `Panda.recover()` calls default stock `flash()`; final signature verification then rejects the wrong variant. The user-authorized native implementation keeps selected firmware identity through DFU recovery/reconnect instead. The real Client recovery regression requires that the signature read still uses `panda_h7_tesla_wake.bin.signed`, and missing selected images fail before any stock read. Mandatory final signature verification is retained. This is an explicit source behavior difference, not claimed raw recovery-trace parity for the optional wake variant.

Source tests/host mocks and successful MCU compilation do not establish physical CAN/relay timing, wake battery use or vehicle safety acceptance. Revised-head CI, complete-package integration, normal device boot/record/upload and driving acceptance remain separate parent gates; this dev-md task performs no device operation.
