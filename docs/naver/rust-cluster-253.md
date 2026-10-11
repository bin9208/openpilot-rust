# Cluster runtime conversion: issue 253

[Issue 253](https://github.com/bin9208/openpilot-rust/issues/253) covers the
registered `carrot_cluster` process under the complete-runtime gate in
[issue 1](https://github.com/bin9208/openpilot-rust/issues/1).
The isolated branch starts at `1d1d121a66f8ea4f4045217c5f8e5a9422b1f37a`.
The registered runtime remains `not_ported`; the policy/packet slice below does
not replace the Python process or establish a device-test candidate.

## Reachable source and implementation slices

`cluster_autorun.py` owns Params gating, display discovery, USB hotplug recovery,
GPU startup/stabilization and encoder fallback. `cluster_run.py` sets locale and
bootstrap scheduling before `cluster/main.py`. The inspected AST import closure
contains 28 entry/runtime modules and 31,560 lines, including conditional route
and navigation inputs. `cluster_ui.py` is a compatibility facade; the separate
lead-label and route-review/validation commands are outside this entry closure.
Private `source-closure.json` records every inspected module/hash/import and the
three cluster-local assets. Renderer fonts/icons also come from the original
selfdrive assets and require separate identities at the rendering boundary.

| Slice | Source-owned behavior | Observable gate |
|---|---|---|
| Autorun/options/rate | live argv, encoder order, HUD gates, GPU waiting, uevent selection, live setting readers | exact source policy plus actual owned Params and autorun lifecycle |
| USB display | TURZX commands, framing, setup gaps, acknowledgement, chunking, recovery, GPU bus exclusion | original/native bytes and operations through owned recipient, disconnect/reconnect and cleanup |
| Inputs/state | live Cereal, VisionIPC cameras, navigation media, radar/model presentation, random/gamepad/route choices | actual owned IPC and unchanged original state transitions |
| Scene/render | cluster models, geometry/caches, themes, layouts, labels, assets and interactions | pinned source/native rendered frames and visual review; main-UI pixels do not establish cluster parity |
| Codec/GLES pipelines | project H264 encoder/decoder bridge behavior, DMA-buffer/PBO/fence/image lifetime and worker cleanup | actual output and failure/lifetime controls against original providers |
| Process integration | actual autorun/cluster entrypoints, settings, shutdown, restart, required CI | owned entrypoint and host/ARM CI, then full startup/upload composition |

Reuse `openpilot_usbgpu::bus_lock::BusLock` with the original
`/tmp/carrot_usbgpu_bus.lock` path and reentrant RAII contract. No shared lock
implementation is duplicated. Reuse public `ui-application::scheduling::Scheduler`
with **core 7**, including child sweeps and offroad little cores. The scheduler's
default core 6 belongs to the main UI. Preserve SCHED_OTHER/nice 19 onroad and
10 FPS, or 5 FPS while `UsbGpuActive`; legacy rate/placement overrides remain
absent. Kernel/firmware, libusb, raylib, EGL/GLES, fonts/assets and external
codecs remain explicit providers. Project-owned C/C++ bridge policy stays in
this conversion scope.

## First policy/packet checkpoint

`rust/crates/cluster` owns the first extracted policy and TURZX packet functions.
The `cluster_policy_trace` example is an oracle recipient, not the daemon.
Source paths are `cluster_autorun.py`, `cluster/main.py`, `cluster_config.py`,
`cluster_usb_display.py` and the bundled
`.vendor/turing-smart-screen-python-main/library/lcd/lcd_comm_turing_usb.py`.
The bundled vendor's GPL-3.0-or-later attribution is retained in the Rust packet
module and crate license.

Packet encryption uses pinned [RustCrypto des 0.8.1](https://docs.rs/des/0.8.1/des/),
MIT OR Apache-2.0, crate archive SHA256
`ffdd80ce8ce993de27e9f063a444a4d53ce8e8db4c1f00cc03af5ad5a9867a1e`.
Its existing `cipher` dependency closure is reused. The original protocol uses
DES-CBC with the original public key as IV, zero padding from 500 to 504 bytes,
then a 512-byte packet with the `a1 1a` trailer. Native Rust owns the header,
timestamp, padding and frame fields. There is no OpenSSL provider mutation.

The first owned comparison observed 542 matching rows: autorun argument and
encoder permutations, uevent product/action precedence and replacement decoding,
FPS/bitrate rounding, camera hysteresis/zoom, command ciphertext and frame bytes.
The source runs unchanged extracted function bodies; source clock/profile
providers are controlled inputs. No USB enumeration, Params access or display
is claimed by this pure boundary.
The initial frame fixture constructed size fields itself. A corrected oracle
calls the actual original `_build_frame_payload` and `_build_h264_chunk_payload`
bodies; only those 36 affected frame cases were rerun and all matched. The other
506 rows are reused. Original PyCryptodome is a read-only cached test provider.
The exact invoked example ELF is
`684ff28bfcd9109394f58f5817ba4aa1c8a6b69af7e6919a1533ec069e0fdb54`.
Private evidence is under primary `.omo/evidence/253-cluster/`:
`policy-build.json`, `policy-first-invocation.json`,
`policy-source-frames-invocation.json` and their raw request/source/native JSON.
Initial strict-Clippy documentation/annotation findings are retained;
`policy-clippy-final.json` records the corrected strict gate.

## Reviewed USB provider integration

The cluster branch imports only the reviewed Rust USB provider from
`f6f301913200edc15d898d672590a48a0603b799`, whose five native owner modules match
the earlier `da9b6a3e` checkpoint. The obsolete project CXX USB bridge is removed;
optional libc/libloading and the required typed allocation error replace its
compile seams. GPU/model runtime changes are not imported. The retained libusb
header and LGPL notices still describe the external ABI.

One new owned library control verifies three terminal callbacks followed by
transfer, stream, interface, handle and context release. The source-identical
provider's prior eight shared-library comparisons and ten ASan cases are reused.
Four Miri observations cover the production callback's owned Cell lifetime,
not the entire foreign USB implementation. `usb-provider-transfer-freeze.json`
binds exact copied source hashes, the selected binary, command output and scope.
The transfer is durable at `9300830f6f8b2b96d7a599038849ffbaaea3b1b8`.

## Additive USB metadata checkpoint

`native_usb::Scanner` owns a retained metadata-only context and returns the first
matching device descriptor without opening a handle. `Usb::configuration(index)`
loads `get/free_config_descriptor` lazily, copies configuration, interface and
endpoint fields, then releases the entire foreign tree. Existing
`Api::system`, `Usb::open`, synchronous transfers and batch ownership are unchanged;
no control GET_DESCRIPTOR requests replace cached descriptor queries.

The original `find_supported_usb_product`, `_find_expected_usb_device` and
`_cache_out_endpoint` execute unchanged with pinned
[PyUSB 1.3.1](https://github.com/pyusb/pyusb/blob/v1.3.1/usb/core.py), BSD-3-Clause,
wheel SHA256 `bf9b754557af4717fe80c2b07cc2b923a9151f5c08d17bdb5345dac09d6a0430`.
Five actual source/native controls cover PID 0092, PID 0123, absence, denied open
and configuration failure. The configured value is 7, and source descriptor order
chooses OUT 07 / IN 84 from alternate 2 regardless of transfer type. Repeated scans
precede any open. Raw warning/wrapped-error strings remain captured; this API
boundary compares the underlying USB error code, not final display error wording.

Six additional native controls reject null/negative descriptor trees and release
an allocated tree even when its query returns an error. All eleven native cases
pass with ASan instrumenting Rust and the owned foreign library; system libusb
is not instrumented. A separate actual control verifies discovery/open against
the original owned library lacking both optional configuration symbols.
Three copier tests pass all four Miri levels, including copied data after input
storage expires. Miri metadata stripping was disabled only for those checks after
dependency E0463 failures. Its first executable run found a stale shared pointer
in a mutable test input; the test now borrows each immutable tree afresh. Failure
outputs remain in `metadata-miri-*.json`.

Private receipts are `metadata-comparison-invocation.json`,
`metadata-comparison/result.json`, `metadata-allocated-error/result.json`,
`metadata-asan/result.json`, `metadata-lazy-symbols/invocation.json` and
`metadata-miri-level-{1,2,3,4}.json` under primary `.omo/evidence/253-cluster/`.
These metadata controls do not cover display setup, frame transfer, hotplug or recovery.

## Owned display transport checkpoint

The cluster crate now owns display connection, cached endpoint selection,
initialization, brightness/orientation/rate updates, JPEG/PNG upload, H264 setup,
chunk negotiation, acknowledgement/queue flow control, bounded draining,
disconnect detection and reset/reconnect behavior. It retains a metadata context
across reconnects like the source's retained PyUSB backend. Partial endpoint-cache
failure remains an owned state, and the slow acknowledgement error path preserves
the source's repeated connection-state notification. Every USB operation and
handle release uses the existing shared bus lock.

The owned transport recipient presents an active alternate-0 bulk interface.
It is separate from the metadata alternate-2/non-bulk ordering fixture. Unchanged
original display methods execute with actual PyUSB 1.3.1 and the original vendor
packet function bodies. Thirty-four source/native scenarios match command,
frame, transparent overlay and H264 bytes, clear/read/write/reset ordering,
requested sleeps, dimensions, setting results and connection-state updates.
They include slow/fast and acknowledgement choices, both discovered products,
initialization/access/configuration failures, partial endpoints, three-error
recovery, short writes, empty/time-out acknowledgements, successful bounded
drains, optional clear failures, input/I/O disconnects, intermittent frame
failures, eGPU reset protection and H264 sizing/pacing/status behavior.

The comparator normalizes each `Err` payload to failure status; it does **not**
assert identical error strings or error classes. Raw messages are captured in
each source/native invocation and result. Clock and Params effects are controlled
inputs here; this does not prove real Params, local-time or profiler integration.
The source harness retains actual discovered PyUSB devices solely for final
resource disposal after source disconnect has cleared its display reference.

The invoked native ELF is
`fc157cb462568c8f79c95f1f5acad1cdedaf0bd4f3e488a9ec582cac73e548e9`.
Receipts are `transport-final-invocation.json`, `transport-final/result.json`,
`transport-clippy-final.json`, `transport-package-test.json` and
`transport-freeze.json` under primary `.omo/evidence/253-cluster/`.
Strict Clippy/all-target checks and the package test command pass; the behavioral
proof is the owned-recipient comparison, not the empty unit-test suite. No new
unsafe Rust is added. The source overlay's level-4 zlib output is reproduced by
the already pinned external `zlib-rs 0.6.8` provider.

## Params and live-clock composition checkpoint

`usb::ParamsRuntime` constructs a fresh existing Params/UI FIFO store for every
source helper. Reset protection reads loading and, when needed, active from the
same owner. It flushes each connection update before the helper returns, matching
the destructor of the source's temporary Cython Params owner. Boolean key
read/open failures become empty values like original C++ `read_file`; constructor
failure returns false for pacing and true for reset protection. Loading and active
flags retain separate decisions. Wall-clock command timestamps use the
source floating subtraction from local midnight through pinned `chrono 0.4.45`.

The reused original Cython Params ELF is
`4b71d2b2cc5117f4e62fb55ece3c305e1fd09e4934c3a8af993010344e1f684d`;
all recorded source hashes match the current tree. Thirty-eight physical-file
controls compare unchanged source helpers with Rust, including absent, empty,
strict boolean, malformed and directory-valued flags and completed atomic
connection writes. Three live-clock controls bracket the native value with the
actual vendor timestamp expression in UTC, Asia/Seoul and America/New_York.
These clock controls do not establish historical DST ambiguity/gap behavior.
Three owned-file Rust tests pass after their missing-API RED run.

Two actual source/native lifecycle controls reproduced a review finding before
the repair: removing the namespace after runtime construction allowed the source
connected helper to recreate it, while the native persistent owner failed to
write; replacing the root with an ordinary file protected reset in source while
native returned false. Both controls now match after moving constructor ownership
to each helper. Global Params get/put error and recovery semantics are unchanged.

Four additional cases compose actual original/Rust Params and live clocks with
the owned USB recipient: idle, active eGPU, recovery while loading, and disconnect.
They match state, operation order and packet content. Real packet timestamps are
separately checked against source clock brackets and removed only for the packet
comparison. Failure strings remain captured and are normalized to failure status.
No physical display or vehicle is contacted.

Private receipts are `params-lifecycle-red-invocation.json`,
`params-lifecycle-red/result.json`, `params-lifecycle-green-invocation.json`,
`params-lifecycle-green/result.json`, `params-runtime-final-invocation.json`,
`params-runtime-final/result.json`, `params-usb-final-invocation.json`,
`params-usb-final/result.json`, `params-runtime-test-final.json`,
`params-runtime-clippy-reviewed.json`, `params-runtime-ruff-reviewed.json` and
`params-runtime-freeze.json` under primary `.omo/evidence/253-cluster/`.
The Params recipient ELF is `103938dec87e6b501809933b1138641202f8e1ab169fb84aaca6943c23f15083`;
the composed USB recipient ELF is `758abd5735fd1172064c982add5e2a9567da9ad95d3dc0a76ff6f50c8fc869e4`.
The cluster now imports the existing UI application library for its Params store
and subsequent core-7 scheduler composition; its locked raylib provider/header
remains an explicit build dependency. This checkpoint does not implement the
registered autorun entrypoint or its complete runtime closure.

## USB profiling composition checkpoint

The actual Display owner now collects the source USB timing stages for command
preparation/write/acknowledgement, JPEG/PNG payload construction and uploads,
transparent H264 setup frames, and H264 chunks. Enabled/disabled and clear
operations retain the original collection behavior. Only completed stages append
samples; missing endpoints fail before sampling like source. The command I/O and
frame methods are separate modules so these hooks do not grow one transport file.

The same 34 owned USB cases, plus disabled and clear/toggle controls, match exact
stage names and order, wire operations, packet bytes and state. Source and native
stage durations use real monotonic clocks and are checked as finite/nonnegative;
their elapsed values are not compared for equality or treated as CPU savings.
The initial profiling comparison exposed two endpoint prerequisites that sampled
native work after source had rejected it; the corrected full 36-case run passes.
The four actual Params/live-clock/USB composition cases and strict package,
Clippy, format and Ruff checks also pass. The profiling recipient ELF is
`0e9d6ba1fd49df9a2f9b87e275167ece15e8c5e18082b91aeb1b5fdb9bb6f5de`.
Receipts are `usb-profile-{red-build,first-invocation,final-invocation,test,clippy}.json`,
`usb-profile-final/result.json`, `usb-profile-params-invocation.json`,
`usb-profile-params/result.json` and `usb-profile-freeze.json` under primary
`.omo/evidence/253-cluster/`. Runtime profiling summaries and encoder/render/input
timing hooks will compose with their respective owners.

The registered process, actual live IPC, USB hotplug/autorun composition, rendering,
codec/GLES lifetime, nondefault inputs, complete entrypoint, host/ARM CI and AGNOS
packaging remain open. No C3X, NAS, physical USB display or vehicle is contacted.
Full normal startup and existing log upload must be composed before the user's
first device comparison.
