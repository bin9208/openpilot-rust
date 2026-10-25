# Original encoder builder evidence

The unchanged `rust/tools/build_encoder_source.py` from base
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a` has SHA-256
`f9de8259b088c11f180781221dd455b7e8ba8678aad99f3d536725db27bc62e3`.
`original.json` records 19 independently executed original cases, including
compiler commands, generated files, output collisions, disk admission,
failure artifacts and argparse behavior. `streams-original.json` records raw
output from real controlled compiler processes, including failed execution.
Private capture scripts and actual-build evidence are indexed under
`.analysis/scratch/2026-10-15-encoder-builder/`.

The original calls `Path.glob` without sorting. Tests require the same current
filesystem enumeration order, exact original membership and argument slots;
they do not require another filesystem to reproduce the capture host's order.
Fixed source, include and link ordering remains exact. The interpreter-only
service-generator failure disappears when the compiled service catalog replaces
that invocation; the service-header filesystem failure remains covered.

Fresh original and native C++ builds matched the entire executable SHA-256
`126ed49990c66b0e93746578d5ec73f6ef582f3216e43641518cddc97fa1658a`.
Generated headers, source hashes, commands and logs matched. Free disk space is
measured separately; native generator provenance is an additional receipt.
This validates the host source builder, not on-device encoding performance or
the complete Python orchestration that still surrounds it.
