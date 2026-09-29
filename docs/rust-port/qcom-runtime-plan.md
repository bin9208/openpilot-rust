# Native QCOM model execution

Issue: [#6](https://github.com/bin9208/openpilot-rust/issues/6), within the
full runtime port tracked by #1. The CPU execution increment is merged in
cc0ca7f0943bbc1cf1c1f0ec1ea82bc431b8e672. It does not replace device GPU execution.

The reference is the pinned `tinygrad_repo/tinygrad/runtime/ops_qcom.py`
QCOMCL a630 path. Preserve its kernel image, constants, argument ordering,
texture descriptors, private/shared memory calculations, cache operations and
dispatch words. IR3 and other GPU generations require separate contracts.

1. Compile small buffer, scalar, image and constant kernels with the pinned
   AGNOS QCOM compiler. Run the original parser, argument builder and queue
   encoder using host-owned memory and fixed synthetic GPU addresses. Save
   their exact outputs, source and compiler provenance as reproducible fixtures.
2. Add checked Rust parsing, argument serialization and command encoding.
   Compare them against every original fixture and reject truncated inputs,
   malformed records, unsupported types and overflowing address/dimension math.
3. Add Linux KGSL allocation, mapping, context, submission and wait ownership
   behind a small syscall boundary. Check ABI sizes/offsets against the pinned
   generated KGSL definitions and test failure cleanup with a fake transport.
4. Extend the build-time graph exporter and Rust executor to own QCOM buffers,
   aliases, kernel arguments and ordered calls, preserving persistent state.
   Python remains an export/oracle dependency only.
5. Validate actual model artifacts offline, all host checks and ARM builds;
   obtain independent review, exact-head CI, merge and post-merge evidence.

No device connection, install, reboot or vehicle test is part of this increment.
Host packet equality and emulated ARM compiler execution cannot establish GPU
numerical equality or CPU savings. The first device-test handoff remains gated
on the entire project-owned runtime conversion and normal startup/log/upload
support described in `design.md`. Continue the remaining runtime work after
this increment; keep #1 and #6 open until their acceptance criteria are met.
