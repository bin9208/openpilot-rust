# Native Jetlink owner and driving adapter

Issue: [#44](https://github.com/bin9208/openpilot-rust/issues/44), under #1/#6.
Provenance: project-owned `openpilot/selfdrive/modeld/jetlink/*.py` and MIT
Jetlink revision `f10f4705243812518e6441dfb06bf2178c408310`; its existing
`third_party/jetlink/LICENSE` is retained.

Scope: opt-in `jetlinkd-rs`, native FunctionFS/v2 peer, local RPC, model scalar
adapter, source selection and the Rust driving loop integration. Production
manager selection is unchanged. This is a host-validated component of the full
runtime conversion, not a device-test candidate or measured vehicle improvement.

## Preserved boundaries

- `cinque-v3.json` is included directly from the unchanged original source.
  Original artifact SHA, ORT 1.22.0, protocol 2, dimensions, finite inputs/outputs,
  generation, frame, sequence and reset checks remain enforced. No engine upload
  or alternate model is introduced.
- The original stopped acknowledgement, explicit activation edge, restart loss
  latch and exact device acceptance record remain required. An external result
  never clears a loss merely because native inference succeeds.
- One background socket worker overlaps continuously executed native inference.
  Shadow never waits. Active uses the remainder of the complete 50 ms frame
  budget, including warp, inference, parser and publication. Late fallback
  invalidates modelV2, drivingModelData and cameraOdometry together.
- Driving integration calls the already compiled original `prepare` entry and
  reads its `warped` output. Actual Jetlink warp is restricted to the original
  QCOM backend and fixed 393,216-byte geometry. CPU fixtures cannot authorize a
  real active Jetlink path. The native output parser is reused; Cinque's four
  raw action values remain on the wire while the existing action policy consumes
  the first two, as the original runtime does. `SEND_RAW_PRED` with active Jetlink
  preserves the inherited missing `raw_pred` failure as the dedicated nonzero
  `JetlinkRawPredictionsUnavailable` error; the ordinary non-raw path succeeds.
  The original KeyError is reproduced and separately tracked in [#49](https://github.com/bin9208/openpilot-rust/issues/49). Native bytes are never labeled as external.
- FunctionFS opens endpoints only after controller configuration, retains 16 KiB
  reads, 16 KiB burst padding, the 8 MiB queue bound and original descriptor bytes.
  Hardware reads remain on an independent worker. Signal masking prevents
  interrupted USB transfers being automatically duplicated. Background RPC runs
  SCHED_OTHER with widened affinity; the reader requests the original FIFO 51.
- Offroad/eGPU gating and the explicit Off retry acknowledgement remain in the
  owner. The fixed provisioning policy is implemented by the native
  `--setup-gadget` entry point, invoked via sudo with a 15-second bound. It checks
  root, offroad, both eGPU Params, USB bridges and other gadget ownership before
  writing configfs attributes. Standard mount/mountpoint/id utilities remain OS
  interfaces. Neither Rust binary invokes Python or a project-owned Bash helper. Linux kernel/configfs/FunctionFS, libc, the original compiled model
  kernels, and the Android platform implementation are explicit dependencies.

## Reproducible checks

From the repository root, with the pinned Rust toolchain and original Python
oracle dependencies available:

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-jetlink -p openpilot-driving-modeld --all-targets --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-jetlink --examples --locked
PYTHONPATH=. python rust/tools/check_jetlink_reference.py rust/target/debug/examples/jetlink_probe .omo/evidence/jetlink
PYTHONPATH=. python rust/tools/check_jetlink_rpc.py rust/target/debug/examples/rpc_probe .omo/evidence/jetlink
cargo build --manifest-path rust/Cargo.toml -p openpilot-driving-modeld --example jetlink_raw_probe --locked
PYTHONPATH=. python rust/tools/check_jetlink_raw.py rust/target/debug/examples/jetlink_raw_probe .omo/evidence/jetlink
```

The state/adapter oracle imports the actual original implementation. It covers
40,000 state decisions, 500 packing frames, 500 ownership operations, 20 complete
Cinque parser frames, malformed acceptance records, FunctionFS descriptors and
USB framing/padding boundaries. Decisions and bytes compare exactly. Float32
parser bounds were fixed before running at absolute/relative 1e-6; observed
maximum absolute difference was 1.1920928955078125e-7.

Native tests use actual Unix sockets and ordinary FIFO files only at the USB
hardware seam. They cover both source-selection modes, timeout/stale fallback,
malformed generations, reconnect, saturated accept queues, disconnects, frame
ordering, bounded socket/worker stop, offroad refusal, process ownership locks,
and SIGTERM. Separate interoperability runs use the original Python ProxyClient
against the Rust owner and the Rust ProxyClient against the original Python owner
and strict client. Host ASan runs cover the scheduler/signal, socket and FIFO
paths. Miri rejects the unsupported `sigfillset` FFI; that run is retained as a
limitation, not counted as a pass.

The driving regression runs through the original native VisionIPC/msgq peers and
actual compiled original models at 1344x760 dual/road/wide and 1928x1208 dual.
Its source oracle now executes the original Jetlink Off-status publication lines
as well. Four scenarios passed: 90 messages, 57,682 compared fields, unchanged
raw bytes and exact discrete decisions (float32 1e-6, polynomial 2e-5 bounds).

GitHub CI repeats source oracles, native tests, ASan and generic aarch64 builds.
Local evidence is indexed in `.omo/evidence/jetlink/evidence.json`; a local green
run does not replace exact-SHA required Actions checks or parent review.

## Remaining acceptance

No real gadget was configured, no C3X/C4 was contacted, and no Android inference
or QCOM Jetlink adapter was executed on a device. The FIFOs cannot establish
kernel driver recovery or USB latency. Generic aarch64 build/emulation proves
neither the AGNOS hardware ABI nor the 30-minute measured acceptance record.
The original validation gate therefore stays intact. Manager selection, normal
full-runtime startup/log upload, the remaining eGPU path and the rest of the
project-owned conversion remain under #1; no early device handoff is requested.
