# Radard IPC startup cadence fixture (#328)

Issue: https://github.com/bin9208/openpilot-rust/issues/328

At `0ae995891dab8a3ce4f170799a585b1be6ede58e`, [Rust job115875954383](https://github.com/bin9208/openpilot-rust/actions/runs/38570954059/job/115875954383) failed only at hyundai-corner radarState `[1].valid`: original false, native true. All other fields/frames matched. The same fixture passed all six scenarios and lifecycle at430ec72b ([job115860105438](https://github.com/bin9208/openpilot-rust/actions/runs/38566168156/job/115860105438)); unchanged fixture SHA256 `ff3e0b3dab6459a61782fcf78c7d2329a89ed2dd5a5ab9cd7115a78e307a6674`.

The fixture compared separate daemon startup frequency histories driven by receive monotonic clocks. Input timestamps and nominal50ms publisher deadlines do not synchronize those clocks. An actual original/native State comparison with identical clocks demonstrates both implementations return startup validity `[false,true,true]` for50/50ms intervals and `[false,false,true]` for65/35ms. This reproduces the observed shape without a runtime algorithm difference. The failed CI did not capture per-topic receive times, so the specific dispatch hitch remains unmeasured.

## Change

Actual IPC now sends exactly20 fixed preconditioning frames in each mode, requires the final three publications valid, then compares all40 measured scenario frames including event.valid. Preconditioning uses clean scenario0 content with timestamp indexes−20..−1. The initial no-model tracks packet is earlier still (−21). All160 measured input packets retain their exact original430 bytes/timestamps; their concatenated SHA256 is `9b52421501fc4cd5487623d1b61c5489067d2e6e6f1862e4fe82c78df53620ad`, calculated independently from the original430 constructor before comparison. Original zero-model timestamps and stale-track injection remain. No-model/model-absence assertions, invalid18/recovery19, availability loss/recovery and signal expectations remain.

A separate bounded preconditioning/timing receipt is written even on failure, after child cleanup. Timing records include the send attempt before waiting for output, at most60rows. Receipt write failure cannot bypass killing/reaping the owned child. No adaptive frame count or rerun-until-pass is used.

The existing CI availability checker also executes the new `radard_startup_state.py` identical-clock oracle. It compares original SubMaster with the actual unmodified native `state_probe`: all four topic names and alive/frequency/valid flags, all four aggregate checks, explicit startup sequences, intentional invalid/recovery, and the strict0.5s tracks deadline ±1µs. Request, source, native and summary receipts are persisted.

No production runtime or source oracle implementation changed; thresholds and simulation policy are unchanged. Actual later IPC timing remains OS-driven, and arbitrary later scheduling stalls can still affect frequency validity. This is a fixture stabilization with separate startup-semantic coverage, not a measured device/runtime improvement.

## Validation (2026-10-14)

Canonical environment `/home/fma1/.config/openpilot-rust/env.sh`, incremental0/jobs2. Before each focused build the capacity guard reported397.6GiB free against26GiB required. Unmodified `openpilot-messaging --example state_probe` built5.42s; `openpilot-radard --bin openpilot-radard` built10.35s. No other Cargo ran concurrently.

- `python -P rust/tools/radard_availability_state.py --binary /home/fma1/rust/cache/cargo-target/debug/examples/state_probe --output OUT`: original5deadline frames +15identical-clock frames PASS. Startup smooth/hitch/invalid-recovery and deadline expectations asserted, actual native topic set exact.
- `python -P -m unittest discover -s rust/tools/tests -p test_radard_ipc_preconditioning.py -v`:3tests PASS. Original160packet hash, strictly increasing preconditioning→measured timestamps except retained11/22 anomalies, explicit original anomalies.
- `python -P rust/tools/radard_ipc.py --python /home/fma1/rust/openpilot-rust/.venv/bin/python --binary /home/fma1/rust/cache/cargo-target/debug/openpilot-radard --binding BINDING --output OUT`: all6scenarios PASS on first run;240complete measured publication pairs,20fixed preconditioning frames per mode/case, last3valid, strict validity and field parity. Source/native availability validity lists matched exactly.
- `python -P rust/tools/radard_lifecycle.py` with the same binary/binding: all8source/native lifecycle cases PASS, preserving declared numeric-error boundary.
- Controlled real-source failure with warmup count1: bounded readiness assertion fails, false warmup receipt saved, owned child reaped. Controlled receipt-write OSError: owned child already reaped. These controls are ignored scratch evidence, not runtime modes.
- Ruff Python3.12 targeted checks PASS; radard_ipc retains its preexisting ISC002/ISC004 command string concatenation and was checked with those two codes excluded. `git diff --check` PASS.

Host native binary SHA256: `41faf308056319c6999eb2605c183699ef185333e08f259ad0d677bf94d9515c` (radard), `c4518b26239fff4b635ca92f9322fb32c39234d8b872c8491559fe69f00c36bf` (state_probe), built from parent baseline `ab7f1c27` runtime sources. Existing original Params/msgq bindings were reused from `.analysis/scratch/2026-10-14-generation-287/`; no dependency install. Detailed receipts and negative-control scripts: ignored `.analysis/scratch/2026-10-14-radard-ipc-startup/`.

Fresh exact-SHA CI remains required. No device connection or vehicle acceptance was performed. No CPU savings claim.
