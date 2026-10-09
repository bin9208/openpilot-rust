# USB/AMD host-oracle alignment correction (#260)

Tracking: [#260](https://github.com/bin9208/openpilot-rust/issues/260), with
the remaining native runtime in [#154](https://github.com/bin9208/openpilot-rust/issues/154).
This changes the owned host fixture, not GPU instructions or Rust arithmetic.

The original fixture completed 473 native kernels but produced 1,149 non-finite
prediction values; its source replay then exceeded the 20-minute host bound.
The first sampled negative infinities at kernel 308 matched an attention-mask
literal and were not treated as a defect. Captured kernel 310 had no NaN input,
but eight complete-output NaNs retained old bytes at the same stride-six positions.
Its unchanged source shader reproduced all 4,608 bytes, including those NaNs.

Eight actual memory-instruction controls isolated DWORD stores flooring their
byte address in the host emulator. The actual startup trace recorded all sixteen
SH_MEM_CONFIG writes in mode 3. The configured UNALIGNED semantics apply to GLOBAL
memory according to [AMD RDNA4 ISA §3.3.4](https://www.amd.com/content/dam/amd/en/documents/radeon-tech-docs/instruction-set-architectures/rdna4-instruction-set-architecture.pdf).
The explicit fixture adapter rejects other modes and expresses each 32-bit store
as four ordered existing byte stores. The source/vendor cache is unchanged.

The same eight controls passed with the adapter. Captured kernel 310 then produced
2,304 finite half values in 0.580 s (original replay 0.550 s). A single corrected
full comparison completed in 1,615.700 s: 473 Rust-dispatched and 473 original-C
dispatched kernels; all 18,452 prediction values were finite, and predictions plus
all three recurrent buffers matched byte for byte. The immutable launch executable
was `120ee5f63f391316ba10486f01a770c7704108c4013c8340d1f8125276e943ba`.

The scoped comparison shares native-loaded buffers and the configured host
emulator; original replay resets aliased recurrent inputs to zero. It does not
independently validate the loader, physical GPU behavior, performance or devices.
Native provisioning/boot policy and the ONNX graph-lowering/JIT/compiler/serialization
path remain implementation work. Earlier failures remain recorded.

Current ignored evidence under `.omo/evidence/154-runtime-resume/`:

- `unaligned-store-red-v2.json` and `unaligned-store-byte-candidate.json-v5`.
- `kernel310-source-replay-v2/result.json` and `kernel310-byte-candidate-v5/result.json`.
- `model-numeric-byte-v1/{result,source-comparison,global-store-config}.json` and raw outputs.
- `model-numeric-byte-checkpoint.json`, including the exact invocation, phase counts,
  output hashes, launch input identities, cleanup observable and limits.
- `numeric-fixture-checkpoint/` for the owned file list, hashes and Python gates.

Raw model weights, captures and executables are not committed. Historical raw
evidence removed by the user's cleanup is not reconstructed or used as current proof.
