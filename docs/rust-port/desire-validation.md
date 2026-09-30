# Driving-model desire state

Tracking: [#24](https://github.com/bin9208/openpilot-rust/issues/24), within
[#1](https://github.com/bin9208/openpilot-rust/issues/1) and
[#6](https://github.com/bin9208/openpilot-rust/issues/6).

`openpilot-desire` ports the source `DesireHelper`, active `desire_lib` modules,
and Bluetooth `CommandReader`. It preserves lane/edge interpolation, hysteresis,
side-object and blindspot retention, receding-track confirmation, maneuver
classification, driver/ATC conflict handling, lane-change/turn states, steering
cancellation, trailer gates and parameter refresh every 100 model frames.
Production model daemon selection and settings remain unchanged. The driving
daemon still needs to connect this library to actual model and cereal inputs.

The source is the independent repository's original Python implementation at
`6f55d6c6527d8e824f67d9cec53c278e9af0ed6b`. The scalar interpolation preserves
NumPy's alternate-direction calculation and equal-value fallback for infinities,
following [NumPy compiled_base.c](https://github.com/numpy/numpy/blob/b832a09cf2a169c833dd2371e7c07aa00b293242/numpy/_core/src/multiarray/compiled_base.c#L688).
The crate retains the NumPy copyright/license in `NUMPY-LICENSE.txt`.

## Observable validation

`check_desire_reference.py` invokes the actual source helper and dependencies.
Only Params reads and remote-command input are supplied by deterministic fixtures;
the timestep is extracted from the original realtime module so importing hardware
management is unnecessary. Rust receives the same full geometry, car, navigation
and radar inputs. Every serialized helper/side state, command-allowed flag and
parameter-refresh decision is compared after every frame.

Forty 600-frame randomized sequences, complete left/right lane-change cycles,
and five focused receding-track/BSD sequences cover 24,585 frames. All discrete states match. After enabling correctly rounded
JSON float parsing, maximum numeric difference was zero against a predefined
1e-10 bound. Coverage includes all four lane-change states, both turns, both
lane-change desires, trailer/ATC/driver gates, and positive/negative parameter
modes. The comparison does not infer branch coverage from randomized frame count.

`check_command_reference.py` runs both readers against actual temporary journal
files over 1,026 reads. Returned command, consumed identifier and repeat state
match exactly (381 returned commands). Cases include startup leftovers, expiry,
future timestamps, 20 ms throttling, duplicate consumption, disallowed consumption,
learning/cancellation, legacy single messages, malformed structures, the last-64
message limit and 128-entry seen-ID eviction.

The first journal comparison found that default serde_json parsing moved a time
by one float64 ULP, changing a 20 ms boundary decision. A regression test failed
with the default parser and passed with `float_roundtrip`; the full reader
comparison then passed. A second regression reproduced the NaN-versus-infinity
interpolation mismatch before the NumPy fallback was preserved. Both fixes retain
source thresholds and behavior.

Reproduce after building all `openpilot-desire` examples from `rust/`:

```sh
PYTHONPATH=. python rust/tools/check_desire_reference.py \
  --binary rust/target/debug/examples/desire_probe --output /tmp/desire-comparison
PYTHONPATH=. python rust/tools/check_command_reference.py \
  --binary rust/target/debug/examples/command_probe --output /tmp/command-comparison
```

The output directories must be new. CI runs both comparisons in addition to Rust
tests, Clippy, formatting and ARM builds. Exact revisions and Actions results are
recorded in the issue after completion. No device was accessed; these results do
not establish vehicle acceptance, CPU savings, complete daemon integration, or
the full-runtime startup/logging/upload handoff required by `design.md`.
