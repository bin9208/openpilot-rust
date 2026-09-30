# Driving-model input state

Tracking: [#20](https://github.com/bin9208/openpilot-rust/issues/20), within
[#6](https://github.com/bin9208/openpilot-rust/issues/6) and the full-runtime
delivery gate in [design.md](design.md).

## Scope and source

`openpilot-modeld` now owns the camera selection/pairing, dropped-frame state,
calibration transforms, desire rising edges, traffic convention, action times,
and previous-feature input packing used by the driving-model loop. This is an
input-state library; connecting the complete driving daemon remains outstanding.
Production daemon selection is unchanged.

The reference behavior comes from `openpilot/selfdrive/modeld/modeld.py`,
`camera_sync.py`, `helpers.py`, and `common/transformations/{camera,model,transformations}.py`
at independent repository baseline `6f55d6c6527d8e824f67d9cec53c278e9af0ed6b`.
Camera pairs retain the existing 20 ms limit and ten resynchronization attempts.
Dropped frames still select prepare-only mode during the first ten frames even
though the reported filtered drop ratio is zero during warm-up. Resetting warm-up
does not erase the last camera frame identifier or desire history.

Calibration updates require an updated calibration message and previously seen
road-camera/device messages. The existing device/sensor table and yaw-trim gate
are preserved. Matrices retain the original float32 trigonometry, float64 matrix
operations, inverse residuals, and final float32 conversion.

NumPy 2.4.6 uses a polynomial for x86 SIMD float32 trigonometry which can differ
from libc by one float32 ULP. `numpy_trig.rs` follows NumPy source revision
`b832a09cf2a169c833dd2371e7c07aa00b293242`, with its copyright and BSD license in
`NUMPY-LICENSE.txt`. The x86 AVX2/FMA host path was compared against the installed
NumPy wheel. ARM uses the platform scalar functions; actual AGNOS NumPy/libc
equivalence and the AVX512 dispatch have not been measured. These host results
are not device validation.

## Reproducible comparison

Build `input_probe` with `cargo build -p openpilot-modeld --example input_probe`
from `rust/`, then run from the repository root with its Python dependencies:

```sh
PYTHONPATH=.:tinygrad_repo:rust/tools python rust/tools/check_model_inputs.py \
  --binary rust/target/debug/examples/input_probe --output /tmp/model-input-comparison
```

The reference executes the actual source camera helper and extracted original
model-loop assignments. It compares stream selection, receive counts and camera
metadata, dropped-frame decisions and filter state, packed inputs, and retained
calibration updates, including unknown cameras and non-finite angles.

Local results on 2026-09-30: all 8,010 camera-pair calls and 2,000 input frames
matched. Of 90,000 calibration entries, 89,992 matched by bits or were NaN on both
sides; the remaining eight were signed-zero differences. Maximum finite absolute
error was zero, within the predefined 1e-6 absolute/relative bound. Six Rust tests
cover the state transitions and malformed feature counts. The initial tests
failed before the modules existed, then passed after implementation.

`check_input_pipeline.py` additionally feeds Rust-generated transforms and packed
inputs into the native executor, using actual original compiled driving models.
For each camera size (1344x760 and 1928x1208), twelve frames include changing
desires, traffic convention, action times, calibration, and one prepare-only gap.
Features from the model feed the following frame and remain retained across the
gap. All 144 output files (model, warped images and recurrent queues) matched by
bytes; all 432 calibration entries and packed/drop states matched exactly.

The first pipeline comparison exposed an oracle build mismatch: the local
original artifact contained LLVM 20 machine code while the native export used
LLVM 18. One to four tail action values differed by at most 9.536743e-7. Recompiling
the source with LLVM 20 reproduced the original kernel's complete machine code
and restored exact output equality. The comparison keeps exact hashes and fails
on mismatches. CI compiles the original and exported kernels in the same LLVM 18
environment; it does not reuse the local LLVM 20 artifact.

Exact-commit CI results are recorded in the issue as they complete. These tests do not demonstrate
normal startup, route logging/upload, CPU savings, or vehicle behavior. First
device handoff remains gated on the entire project-owned runtime conversion.
