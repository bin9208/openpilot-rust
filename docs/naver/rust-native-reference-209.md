# Native reference dependency architecture (#209)

The original Params Cython/C++ comparison builder failed on native ARM runners
at `18d33121da005cb495623124d99a28f031d68a6d`. Its shared json11 staging helper
always selected the locked x86-64 wheel. The compiler and Python were aarch64,
so the linker rejected the x86-64 archive with `Relocations in generic ELF
(EM: 62)` and `file in wrong format`.

The affected exact-head jobs are
[planner](https://github.com/bin9208/openpilot-rust/actions/runs/37167856272/job/111334489501),
[RadarCAN](https://github.com/bin9208/openpilot-rust/actions/runs/37167856272/job/111334489482)
and [Xiaoge](https://github.com/bin9208/openpilot-rust/actions/runs/37167856272/job/111334489496).
Planner artifact `11290061750` retains the compiler stderr in
`_temp/planner-original/params/commands.json`. Native solver generation completed
before that failure. The planner host comparison and pure Miri job passed on
this head; those results do not replace the failed ARM comparison.

`native_logging_build.stage_json11` now selects the lockfile's native host
architecture, retains the existing SHA-256 verification and records the chosen
architecture. Unsupported architectures fail before a download rather than
selecting another architecture. The same helper serves original logging,
bootlog, logger, encoder and Panda references; original C++ and runtime policy
are unchanged.

Local evidence is in `.omo/evidence/native-reference-209/`:

- `selection-red-v1` reproduces the wrong ARM library and unsupported-host
  fallback; the x86-64 and hash-rejection checks already pass.
- `selection-green-v1` passes the corrected selection tests plus affected
  encoder/Panda CI helper tests, 11 tests in total.
- `locked-arm-stage-v1` fetches and verifies the actual ARM wheel SHA-256
  `0f7764609db411d98c540d985c289a2c37f282fbf2a75e2503017f1243546653`.
  Its archive member reports ELF machine AArch64.
- `locked-arm-link-v2` links the actual library to a bounded C++ JSON parser
  check. `locked-arm-execution-v1` runs it under the extracted AGNOS loader
  through QEMU Cortex-A57 and verifies the original parse/access/dump operations.
  The initial local check had the wrong header include path; its failed command
  is retained and the corrected check uses the source's `json11/json11.hpp` path.

Corrected hosted exact-SHA jobs remain required before closing
[#209](https://github.com/bin9208/openpilot-rust/issues/209). This source-builder
repair is not whole-runtime, device or performance acceptance.
