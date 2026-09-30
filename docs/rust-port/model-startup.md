# Model initialization before first-frame reception

Issues [#55](https://github.com/bin9208/openpilot-rust/issues/55) and
[#60](https://github.com/bin9208/openpilot-rust/issues/60), following
[model diagnostics #53](model-diagnostics.md). This is a host-verified correction
to the internal native model path, not the complete startup/upload candidate.

The original driving daemon connects its selected camera streams, reads their
connection dimensions, loads the models, creates messaging and then waits for
CarParams. Its frame loop starts afterward. The original driver-monitoring daemon
also obtains dimensions and loads its model before its normal receive loop.

At frozen diagnostics commit `cb854eded17374ec5ef2b42ad05df6a4d65de28b`, Rust
waited for CarParams and the first driving pair before loading; driver monitoring
loaded only after receiving its first frame. Eight native reproduction cases
(two daemons, two camera resolutions, raw predictions present/absent) confirmed
that original model construction completed with all frames and CarParams
withheld, while the Rust models remained unloaded. A regression expecting source
ordering failed against that frozen binary.

Both Rust daemons now select and load the existing catalog model from the copied
`VisionClient::layout()` immediately after connection. Driving loads before
CarParams, messaging and loop entry. Driver monitoring loads before messaging
and its first `receive`. No camera buffer is polled or consumed during loading.
The prior optional runtime/subscriber/publisher initialization states are removed.
The trusted catalog lifetime, backend choice, QCOM priority, process scheduling,
frame validation, frame pairing and model/recurrent computations are unchanged.

Existing source log messages remain at the actual milestones: `models loaded`
follows successful construction and precedes first-frame reception; driving's
CarParams message follows that load. No readiness message is synthesized. The
first driving runtimeTiming loop no longer includes model initialization because
initialization now happens outside that loop; no time or samples are subtracted.

## Evidence scope

`rust/tools/check_model_startup.py` builds on the original Python VisionIPC
binding and real native camera server. Its reference child executes the original
`ModelState` class and original connection/load statements, with only artifact
paths redirected and the internal CPU backend selected. The driving reference's
single Params Boolean write uses an isolated test directory; it does not emulate
or claim the complete Params/manager/eGPU startup. Source initialization finishes
before the controller sends any frame, and source reception then returns the
first supplied frame ID. A fresh native server is used for the Rust comparison.
The same test withholds all frames, withholds then supplies CarParams, observes
load order and stops the waiting daemon cleanly.

The continuous original-model suites additionally check:

- Both driving resolutions with dual, road-only and wide-only streams, preserving
  the first prepare-only frame and subsequent recurrent state, exact raw bytes,
  existing field tolerances and source logging order: 114 publications and 73,058
  compared fields across six scenarios.
- Both driver resolutions with raw output enabled and disabled, including the
  first frame's full original-model output, subsequent calibration updates,
  queue capacities and signal/error behavior: 14 total frames and 658 compared
  fields, including four first frames (188 fields).
- Real diagnostic collector publications and disk records, plus native
  VisionIPC/msgq sanitizer tests and Rust workspace checks.

The source driver has a fallback for a connection whose width/height is absent.
The existing native boundary rejects zero buffers and invalid dimensions at
connect, so an accepted native connection always has its validated copied layout.
This change keeps that boundary; it does not add an untested fallback, weaken
validation or claim compatibility with invalid/legacy metadata.

Reproduce in the source-locked original-model environment:

```sh
python rust/tools/build_visionipc_python.py --output /tmp/model-native-python
export PYTHONPATH=/tmp/model-native-python:$PWD:$PWD/tinygrad_repo:$PWD/rust/tools
python rust/tools/check_model_startup.py --driving rust/target/debug/openpilot-driving-modeld --driver rust/target/debug/openpilot-dmonitoringmodeld --models /tmp/original-models --catalog /tmp/native-catalog --output /tmp/model-startup
python rust/tools/check_driving_daemon.py --binary rust/target/debug/openpilot-driving-modeld --collector rust/target/debug/openpilot-logmessaged --models /tmp/original-models --catalog /tmp/native-catalog --output /tmp/driving-startup
python rust/tools/check_driver_daemon.py --binary rust/target/debug/openpilot-dmonitoringmodeld --collector rust/target/debug/openpilot-logmessaged --models /tmp/original-models --catalog /tmp/native-catalog --output /tmp/driver-startup
```

`--expect lazy` records the old behavior when explicitly comparing the frozen
baseline. Default `pre-frame` is the acceptance assertion. Original startup uses
`DEV=CPU`; `CPU:LLVM` is a compiler selector used by the artifact build, not a
valid runtime CPU device index. Original models/catalog remain the freshly built
NumPy 2.5.3 artifacts from #53, with their unchanged source and hashes verified.

Parent exact-head CI, actual eGPU paths and their pending diagnostics, complete
runtime startup/log upload integration, and device acceptance remain separate.
No vehicle was contacted. Host loading durations and generic aarch64 builds do
not establish device readiness or CPU savings.

Docs-Not-Needed: internal native model initialization ordering; no user setting or
production daemon selection change.
