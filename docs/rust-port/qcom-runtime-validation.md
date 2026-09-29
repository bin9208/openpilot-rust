# Native QCOM runtime implementation evidence

This is an implementation increment for [issue #6](https://github.com/bin9208/openpilot-rust/issues/6).
The complete runtime remains tracked by #1. No production daemon selects this
backend, and no vehicle has been connected, installed, rebooted or tested.

## Implemented contract

`QcomBundle::load` verifies the version-1 `qcom-cl`/`a630` manifest, allocation
ranges, aliases, bindings, image dimensions, checksums, kernel binaries and launch
contracts without opening a GPU. `QcomModel::load` is an unsafe API for trusted
kernel bundles. It opens KGSL, verifies a630, creates the original context and
power constraint, maps its own buffers and prepares kernel arguments and commands.
The runtime executes no Python and does not compile kernels on the device.

Rust preserves buffer aliases, persistent state and ordered copies. GPU batches
end before a CPU copy. Each dispatch uses the original cache operations, and each
submitted batch waits for its KGSL timestamp before CPU access. The largest
command buffer is allocated during model load and reused across frames. Kernel,
argument, stack and storage allocations remain owned until shutdown. Requested
allocation bytes have a cumulative 4 GiB limit; OS page rounding is additional.

Submission or wait failure closes the context/device and blocks further access.
Cleanup errors are retained with the execution error or reported during Drop.
Normal teardown tries context destruction, releases the file descriptor, then
unmaps CPU mappings. GPU references to pages and cancellation at file release are
KGSL kernel responsibilities. Fake-driver ordering tests do not establish that
behavior on the target kernel.

The exporter accepts captured flat QCOM JIT input/output bindings and host copies.
It preserves the compiled binary, argument types, alias offsets and call order.
Unsupported operations and dynamic symbolic scalars fail explicitly. Fractional
workgroup counts use the original truncated global extent and rounded-up group
count. IR3, other GPUs, nested eGPU bindings and external VisionIPC mapping are
outside this contract and remain implementation work.

## Observed host evidence

- Four authored kernels compiled by the pinned AGNOS compiler under ARM64 QEMU:
  buffer addition, scalar addition, image copy and constants. Compiler provenance,
  source and generated outputs are retained in the QCOM fixture directory.
- Rust metadata, five full argument buffers (including float32/float16 images),
  integer and fractional dispatch words, and memory barriers match the original
  `QCOMProgram`, `QCOMArgsState` and `QCOMComputeQueue` exactly.
- KGSL struct sizes, every used field offset and ioctl request numbers match the
  original generated ctypes declarations. Original fixture re-verification runs
  in CI without the ARM compiler or a GPU.
- Truncation/mutation tests reject malformed ranges without panics; the shader
  load field accepts 1023 units and rejects 1024 before encoding can truncate it.
- Fake drivers exercise memory ownership, copy overlap, recurrent state, batch
  boundaries, wait/submission failure and combined execution/cleanup errors.
- The Python exporter produces a bundle from a synthetic captured QCOM graph;
  the separate Rust CLI validates it and reports `gpu_executed: false`.

Miri runs the existing allocation/graph cases plus the device ownership tests.
The larger pure descriptor mutation corpus runs in normal Rust tests, rather
than consuming interpreter time needed for unsafe ownership checks. Native KGSL
ioctls and GPU memory accesses are not executed by Miri or host tests.

## Reproduction

```sh
cd rust
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cd ..
export PYTHONPATH=.:tinygrad_repo:rust/tools
python rust/tools/qcom_fixtures.py --verify rust/crates/model-runtime/tests/fixtures/qcom
export DEV=CPU:LLVM CPU_COUNT=4 JIT=2
export MODEL_RUN_BINARY="$PWD/rust/target/debug/model-run"
python -m pytest -c /dev/null -p no:cacheprovider --confcutdir=rust/tools/tests rust/tools/tests -q
rust/target/debug/qcom-model-run --check-bundle /path/to/exported-bundle
```

`--trusted-bundle BUNDLE SEQUENCE.json` is the executable QCOM CLI contract; it is
not part of these host tests. Input/output sequence format matches the CPU CLI.
It requires an a630 KGSL device and trusted native code. `QCOM_PRIORITY` preserves
the original default of 8 and accepts the 4-bit priority field.

## Remaining acceptance

Actual-model GPU numerics, target-driver behavior, frame timing, thermal behavior
and CPU savings are unmeasured. A host build, fixture comparison or ARM cross-build
does not establish these properties. Complete offline artifact construction,
the pinned AMD/USB backend, VisionIPC, model daemons and the remaining M3-M6
runtime before the first device-test handoff. Normal startup, logging and the
existing upload comparison flow remain part of that same full delivery gate.

The preceding CPU increment is merged at
`cc0ca7f0943bbc1cf1c1f0ec1ea82bc431b8e672`; its post-merge
[Rust checks](https://github.com/bin9208/openpilot-rust/actions/runs/36630367354)
and [integration gate](https://github.com/bin9208/openpilot-rust/actions/runs/36630367871)
both passed. QCOM head/merge CI is tracked separately in its PR; #1 and #6 stay open.
