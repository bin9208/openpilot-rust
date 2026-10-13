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

## Autorun controller and startup dependency composition

The native autorun owner now retains its source persistent Params owner, reads
the existing HUD/encoder/orientation/debug/onroad settings, and distinguishes
automatic H264 initialization fallback from ordinary runtime failures. Malformed
registered integers follow the actual Cython `std::stoi` fatal boundary: both
processes abort with SIGABRT, while key I/O failure remains an empty value. Eleven
actual-file/source controls pass, including decimal prefixes, missing/directory
keys, completed initialization and namespace removal. Six real-clock startup
controls retain the original 3-second discovery, 60-second startup timeout,
10-second stabilization and 100-ms polls; their unchanged full timeout receipts
are reused rather than rerun for the dependency import.

The reviewed GPU source closure is imported from
`f8cf3a23bfadb0f0e3ee5f07187d282869e67768`, with the shared home resolver repair
from `e8ff61aeb5dc2952ca515974851fc5ab2e2c2c52`. Cargo dependencies/module exports
are unioned; the six cluster metadata additions and metadata fixture remain
unchanged. The UI native initializer also calls the shared `Paths::for_runtime`
resolver. The source cache location is independent of Params prefix; absent HOME
uses the passwd fallback, empty HOME uses `/`, and an empty model override falls
back normally. The cluster probe preserves presence-before-readiness ordering.
Five composed cases use owned USB sysfs files, actual original model state and
chunk readiness, and the current admitted native package: active float-speed
SuperSpeed, previous fallback, USB2 rejection, missing local provider and
precompiled-only blocking previous. They match actual Params startup-wait
decisions. The reused 406165-byte JIT model has a synthetic ONNX identity; this is
not trained-model or physical GPU acceptance.

Ten original/native controller comparisons match exact argv, state and call
order through controlled entrypoint/scheduler/discovery recipients. They cover
mode stop, PC window fallback, board/PC encoder fallback, forced H264 and unplug
five-second retries, output-disabled dim requests, owned-datagram connection,
waiting cancellation and orientation recheck. The source uses the actual Cython
Params reader/writer; retry clocks and sockets are real. Error messages and
tracebacks remain raw captures, not a claim of identical error payloads. The
native platform separately composes the existing core-7 scheduler, shared bus
lock, native scan/dim, kernel netlink owner and readiness probe. Render-thread and
encoder-child scheduling still belongs to their pending runtime owners.

Seven event controls compare the actual kernel group-1 socket factory and owned
nonblocking datagrams, including queue draining, zero timeout, absent events and
8192-byte truncation. Five dimming cases compose the original dimming helper,
PyUSB/native recipients, actual Params and wall-clock brackets: success, sync
retry, missing device, metadata failure and missing vendor directory. USB payloads
match after separately bracketed timestamps are removed; operation order and
final connected-state files match, with clean recipient teardown.

The vendor `cluster/.vendor/turing-smart-screen-python-main/library` directory
remains the original availability gate for native scanning/dimming. Those native
operations execute no vendor Python and read no files inside that directory.
Until the packaging policy changes with equivalent evidence, issue #269 must
retain that directory-presence condition alongside the actual native shared
libraries and model assets. Process locale initialization, renderer/codec
entrypoint, live input owners and complete startup/upload composition remain
pending; the controlled entrypoint does not render frames.

Primary evidence is `.omo/evidence/253-cluster/`: `autorun-params-final/result.json`,
`gpu-startup-final/result.json`, `usb-events-source/result.json`,
`autorun-controller-corrected/result.json`, `autorun-readiness-source/result.json`,
`autorun-dim-source/result.json`, their corresponding invocation receipts,
`autorun-final-tests.json`, `autorun-import-final-clippy.json` and
`autorun-startup-freeze.json`. The earlier observed controller, readiness and
dimming ELFs are respectively `7bac051725f32f4f5c6966df451128c32c18b0b889134da92f821e3938bfe9c8`,
`8dd2912fbfbca413a078ef3f9bc5198f4242d7d14c1b3008012bb60944e45537`
and `29d9e2e2444e696cbde90d1de80492cccb542a13d4f620f35dea84d53c9f3f9f`;
the freeze records final source/build hashes without retaining duplicate ELFs.
Nine owned-file/socket package tests pass. No physical device or production
selection is changed, and no complete-runtime or CPU claim is made.

## Live road-camera ownership checkpoint (2026-10-12)

`camera::LiveRoadCamera` now owns the original road/wide VisionIPC discovery,
first-frame stream switching, 500-ms connection retry, 100-ms polling, 1.2-second
stale reconnect, stride/padded NV12 texture upload and 1.08 contrast. The existing
safe raylib provider admits a separate cluster shader slot. Its filter-only
camera method preserves REPEAT wrapping; the earlier shared smoothing helper
also set CLAMP and produced 32,957 changed pixels. A source-only CLAMP control
removed the difference, then the native filter repair matched the unchanged
source at both 384x192 and 800x480. Twelve existing large/compact shader captures
also match exactly after the cluster slot is initialized.

The same owner selects the original TICI EGL initialization path, retains
validated camera mappings through the new safe `VisionClient::retained_buffer`
view, caches duplicated-FD EGL images by buffer index, binds the current image
before each external draw and releases cached images on stream reset/switch.
The external path does not copy NV12 bytes on receipt. Its third import failure
releases the images/placeholder/external shader and copies the current retained
buffer for source-compatible NV12 fallback. A render-batch flush precedes image
binding so earlier external draws cannot consume the newly bound camera image.
The added CXX methods only filter checked indexed textures and flush raylib;
the existing EGL owner continues to own foreign pointers and duplicated FDs.

An unavailable road stream exposed an independent native provider defect:
the original server closes an unsupported-stream handshake without payload/FDs,
but Rust previously classified that reply as corruption. Only the empty/empty
reply now returns false; malformed nonempty replies retain validation.
[Issue 271](https://github.com/bin9208/openpilot-rust/issues/271) remains open for
integration acceptance. The captured RED control is followed by GREEN owned
mapping/stream tests and the full existing msgq package controls.

The owned original C++ VisionIPC server, original Python camera methods and
native owner match 68 real raylib frames in five scenarios at both sizes:
poll/stale cadence, deferred/cancelled switches, server restart, wide-only input
and absent/delayed discovery. Seven composed EGL/GLES recipient controls match
state, NV12 import attributes, cache/bind/retry/destroy order and zero leaked FDs
over 77 steps, including a stream selection without drawing: old cached images
are released at selection, before the next frame. Thirty-eight
initialization/fallback frames match real NV12 pixels.
Foreign texture numeric IDs are normalized only in the provider-call comparison;
raw calls are retained. Successful external-image controls verify ownership and
call composition, not actual hardware DMA-BUF pixels. No physical camera/display,
vehicle, C3X or NAS is contacted.

This is still an intermediate runtime owner. The complete Cereal/Params to
`ClusterUiState` projection, scene/layout/assets, navigation media, H264 decoder
and encoder control, GLES DMA-BUF/PBO readback pools, render/encoder worker
scheduling, locale and registered entrypoint/startup/upload composition remain
open. The original project encoder/decoder bridges under
`openpilot/system/loggerd/{encoder,decoder}/cluster_h264*` remain conversion
scope; kernel/VIDC, EGL/GLES, raylib, FFmpeg/libx264 and trained/assets providers
remain explicit external dependencies. `carrot_cluster` remains `not_ported` in
the installed catalog; QA examples are not daemon entrypoints.

Current private receipts are under this worktree's
`.omo/evidence/253-cluster-resume/`: `camera-copy-final/result.json`,
`camera-egl-final/result.json`, `camera-existing-styles-final/result.json`,
`camera-current-tests.log`, `camera-current-clippy.log`,
`camera-retained-{red,green}.log`, `camera-wide-only-red.log`,
`camera-clamp-control-final/result.json` and their per-scenario raw frames,
provider calls, state and invocations. The final ledger binds observed ELFs,
source identities in `camera-final-receipt.json` and the current validation scope without retaining historical
duplicate build caches. Host evidence is not CPU savings, AGNOS execution or
vehicle acceptance. Normal startup and the existing upload path remain the full
candidate gate before the user's first device comparison.

## Software H264 output ownership checkpoint (2026-10-12)

`h264::Software` now resolves the original FFmpeg encoder/muxer selection,
constructs the source command, owns/reaps the encoder child and forwards every
stdout read through the existing H264 transport. The output factory constructs
the admitted thread-confined libusb owner on its worker; typed brightness,
orientation and FPS controls remain on that owner. This adds no unsafe Send
implementation or GPU module changes. Stderr retains the source's last 20 lines;
output failures remain visible through explicit close. Inherited stdout/stderr
control pipes first held teardown for 8.11 seconds; the current owner releases
them within the source's three-second stdout and one-second stderr join budgets.
Five owned process/output controls pass, including invalid encoder, missing
executable, recipient failure and bounded pipe cleanup.

Three controls feed 80 actual captured live-camera RGBA frames per implementation
to the original/native FFmpeg owners at 384x192/10 FPS and 800x480/5 FPS. Ack,
soft-ack/eGPU pacing and no-ack stream payloads agree byte-for-byte after encrypted
USB framing is removed. The non-H264 command payloads also match after wall
timestamps are removed. Every chunk obeys the negotiated limit and original
flags; stdout read segmentation can differ. Actual FFprobe decodes 79 frames in
both implementations from the 80 inputs, retaining the unchanged source FFmpeg
startup behavior. Children are reaped, `ClusterHudConnected` clears and the owned
libusb recipient records clean release. This proves the software path with real
external FFmpeg/libx264 and an owned USB ABI, not physical display operation.

The shared target reused a build script from issue 269 that referenced an updater
bridge absent from this checkout. Final validation therefore uses the fresh
`rust/target/cluster-253` directory. Camera controls were rerun there: 68 exact
NV12 frames, 12 existing-style frames and seven EGL controls still pass. Current
all-targets cluster/msgq/startup-ui Clippy passes. The observed private-cache
camera, style and software encoder ELFs are respectively
`5fd728a064f7ebe1032d7f3489c0f23b760e64888c93b2f34a213f09e3c2dfa8`,
`0df72e09b273f1a49f6668cffae0550effc9216e3e6e068ebf6adf581d643819` and
`644b4a3849209449f4b52cee2962e5b92e4edbbe45e3392f2ffcc59cf68a22f8`.
`.omo/evidence/253-cluster-resume/camera-codec-private-receipt.json` binds current
source/provider hashes to per-scenario invocations, rendered PNGs, raw calls,
wire captures, dumps, process outcomes and FFprobe results. Earlier shared-cache
identity receipts are intermediate records only.

The full live Cereal/Params projection, scene/layout/assets renderer, navigation
media decoder, native VIDC encoder and SPS/crop/VUI control, GLES DMA-BUF/PBO
readback, locale and full managed process/startup/upload composition remain
conversion scope. Source manager admission is `carrot_cluster`, not `cluster`.
The installed catalog remains `not_ported`; a software encoder fixture is not the
normal cluster daemon. No vehicle, C3X or NAS is contacted.

## Full scene geometry checkpoint (2026-10-12)

`scene::SceneBuilder` now owns the original lane, road edge, model path, follow
distance, vehicle and radar geometry, source palettes and camera transforms.
Bounded geometry/style caches retain shared vertex ownership and original grid
quantization, ordering and cache-hit behavior. `SceneStateKey` checks the same
38 source fields without allocating during an unchanged-state comparison.
Original per-stage profiling names and event order are preserved.

The unchanged `cluster_scene.py` and native `cluster_scene_fixture` produce
matching complete scene structures in 192 ordered cases. These cover both
themes, five camera choices, surround orbits, lane change/blindspot highlights,
solid/dashed model lanes, short-model fallback, road edges, path metrics/follow
distance, raw corner filtering, front/detected radar merging, invalid samples
and warm quantized caches. Vertex comparisons use absolute tolerance 1e-10 and
relative tolerance 1e-11; labels, colors, collection sizes and profiling event
sequences compare exactly. This is scene geometry evidence, not rendered pixels.

An added signed-zero case reproduced the native total-order comparison choosing
`B+1` where Python selected `A+1`; source-compatible stable numeric comparisons
restore the original label and merge order. The retained RED is
`.omo/evidence/253-cluster-scene/source-profile-first/result.json`.
The full GREEN is `source-profile-green/result.json` in the same directory,
with source SHA256
`c8cf9066fcf439760327b2dd65b125e5480858c6c7d6a73d2af1763c592fa0d0`
and invoked ELF SHA256
`94d8f0e7c8379b5287891f2dd40b7cc2c0cd730e16ace37ab89c48beaf2efe2d`.
The repeatable comparison is `rust/tools/cluster_scene_compare.py`.
The actual cluster renderer, live/media integration and daemon/package gates
remain open; this checkpoint does not change manager admission.

## Navigation policy checkpoint (2026-10-12)

`live::navigation` ports the complete `cluster_navi.py` parser, metadata reuse,
session reset, per-item expiry and speed-limit source precedence. The native
models preserve the full Cereal UInt64 generation, sequence and source timestamp
ranges. Real Cereal source/native comparison passes 37 ordered cases, including
same-metadata changed payloads, session replacement, all expiry boundaries,
disconnection, malformed limits, future timestamps, non-finite floats, bounded
lane lists and route points. Two focused regressions fail before implementation
and pass afterward. Original source SHA256 is
`9842570a00b81eaf78d69e46f5078fabb870dea254fd42d0318015f34171a148`;
the invoked native fixture SHA256 is
`02b2bb46c950dd558cc72653fcb301ca51b6a7823e655a3d034034430f83022b`.

Four navigation display policies (panel visibility, map availability, active
signal and primary signal countdown selection) additionally match the unchanged
renderer methods in 770 cases. These include every combination of five optional
signal flags and three remaining-time patterns, plus individual navigation
items and map connection/stall/presence combinations. Two display-policy
regressions have a recorded RED/GREEN transition. Their fixture SHA256 is
`5cdb1d63cd1db509511f9694c9489433acdbb3819c9cfe22aaf538d7d1888bf4`.

Repeatable harnesses are `rust/tools/cluster_navigation_compare.py` and
`rust/tools/cluster_navigation_policy_compare.py`; results live under
`.omo/evidence/253-cluster-navigation/`. These are host state/policy results.
Pixel rendering, real-time input composition and normal daemon/package
acceptance remain separate open gates.

## Shared model and live parser checkpoint (2026-10-12)

The cluster now exports all30 original data records and the148-field state shared by scene and renderer. Source standby/default records, both30-field themes and130 configuration constants are retained. Camera selection helpers reuse the existing verified policy. Retained decoded NV12 frames have checked layouts, owned descriptors and last-reference release; hardware frames cannot be forged or silently erased in JSON host fixtures.

Native borrowed Cap'n Proto parsing matches original production `RouteLogParser`/`frame_to_state` across64 full-state scenarios: car/cruise/gear, speed-limit expiry, plan validity, calibration precedence, device metrics, trip events, model lanes/path/risk predictions, direct lane-change recentering, stale detection handling and recorded radar/CUT-IN/corner displays. Invoked ELF SHA256 is `f136e40813decf1b41230b5b6463503e174a8721e1d7a1739eb9028d7105c38c`; results and exact invocations are in `.omo/evidence/253-cluster-resume/projection-radar-first/`.

`Input::update` and `project_at` now compose native polling, parser state, onroad lifecycle, native navigation freshness and existing HUD presentation. Four focused native tests pass, including actual owned Cereal publishers yielding a full148-field cluster state; captured output is `live-state-second-green.log`. Initial IPC fixture bootstrap was corrected to the existing producer-first setup; no transport/validity policy changed. Params/debug/navRoute/media, optional CAN, VIDC/NV12, scheduling/locale and complete process/startup/upload composition remain open. This is host engineering evidence and does not admit the daemon or establish device benefit.

## System and trip panel rendering checkpoint (2026-10-12)

The native system dashboard, trip report contents and live diagnostic panels
match the unchanged renderer in 96 cases / 192 rendered RGBA frames, with zero
different pixels. The matrix covers both themes, Korean/English and metric/US
units, 1920x480 and swapped 1280x320 layouts, missing values, temperature and
usage thresholds, clamped gauge values, signed calibration angles, trip rounding
and missing/active/inactive live torque data. The parent inspected the actual
Korean trip report capture. This comparison renders real native primitives and
fonts; reference screenshots are only the comparison output.

The source renderer SHA256 is
`fc3e52d6dacb71348b4b5d7fce26b05943fa61f33e58751b0935c9eb284adfdb`;
the invoked fixture SHA256 is
`510aef7b9796ee9697b36c3a76da43acd78efb8aeb47fdd375819fb771e4e027`.
The earlier fixture rejected these surfaces before implementation. Repeatable
cases are `rust/tools/cluster_panel_render_cases.py`, selected with `--suite
panels` in `cluster_navigation_render_source.py`; native comparison uses
`cluster_navigation_render_compare.py`. Receipts are under
`.omo/evidence/253-cluster-panels/`.

The trip render-target cache, complete HUD/3D composition and real-time daemon
remain separate gates. The navigation mismatch and its later complete matrix
result are recorded below. Host rendering is not a device or driving result.

## Navigation and HUD surfaces checkpoint (2026-10-12)

Native RGBA output matches every pixel of the original for 168 navigation cases
/ 336 frames, 152 acceleration/steering/energy-gauge and signal cases / 304
frames, and 144 alert/clock/navigation-badge cases / 288 frames. Matrices include
both themes, Korean/English, 1920x480 and 1280x320, cache-cold/cache-warm draws,
panel swapping, missing values, threshold transitions, unclipped input values,
long alert fitting and synthetic-alert translations. These 928 frames invoke
native fixture SHA256
`2c408de5aa8f154d227a5a4a6ff2560ab4db823d829bfb7f71c07495a3e22438`.
References use unchanged renderer source SHA256
`fc3e52d6dacb71348b4b5d7fce26b05943fa61f33e58751b0935c9eb284adfdb`.

The initial 104 navigation-frame failures exposed texture wrapping that differed
from the source. Cluster image textures now retain default repeat wrapping with
bilinear filtering; the existing product UI's clamped-image API is unchanged.
The last eight failing frames each differed by four pixels because Python's
`rl.Rectangle` fields become float32 before fitting and placement arithmetic.
Native media boundaries and reused navigation rectangles now preserve that
conversion point. A separate source-only clamping probe isolated the first
cause, and the full unchanged-reference matrix passed after both corrections.
No reference pixels or tolerances were changed to obtain agreement.

`SignalState`, `PlotHistory` and effective screen selection additionally match
1,560 original-source cases: 640 screen-mode/navigation/parked combinations,
200 ordered signal/highlight transitions and 720 plot updates including sample
timing, mode reset, bounds and ring eviction. Fixture SHA256 is
`2bae4112972b9794643b98fdee14c385bf3cf6a69f0c79e4f630100088b98ccb`.
Repeatable comparisons are `cluster_navigation_render_compare.py` and
`cluster_hud_policy_compare.py`; complete receipts are under
`.omo/evidence/253-cluster-navigation/render-native-precision-all/`,
`253-cluster-hud/render-first/`, `253-cluster-alerts/render-first/` and
`253-cluster-panels/hud-policy-first/`. These are host surface/state proofs;
ordinary daemon startup, complete output/IPC composition and device acceptance
remain distinct gates.

### Live Params and decoded media ownership checkpoint (2026-10-12)

The complete live update now matches 37 original-source cases across all 148
UI-state fields, including Params-backed navigation images, Unicode/nested JSON,
traffic-light countdown/expiry, external route projection/cache, debug plots and
HUD overrides. Six live tests pass. The source control explicitly disconnects
media in those cases; its frozen fixture is
`32144ed236584ed304913a39857adb38411ae6d8b7fb4225fd9ad61e2bc54ff9`.

The owned non-conflated `carrotNaviMedia` queue matches 18 source projection cases
including UInt64 sequences, session resets, image/config/clear packets and exact
integer-millisecond stale-map behavior. Immutable image/plane bytes are retained
with Arc. The media fixture SHA256 is
`f7694306d3743a7c93bee6bf783d0346919cf0633652209de2122b21c2eb9fd3`.

The additive safe FFmpeg decoder in `encoderd::native::decoder` matches 42 source
requests and 39 decoded frames with exact visible YUV samples and strides,
including 444-to-420 conversion, dimension changes and malformed-input recovery.
The first comparison reproduced PyAV's 16-byte converted-row alignment versus
the wrapper's 32-byte allocation; returned converted planes now retain the
source layout. Allocator row padding is excluded from pixel comparison. The
frozen decoder SHA256 is
`f361a7859f1136ab42ccbca90da90d410261d4f102a83ff44d84ce2fcee5221b`.

An actual IPC publisher/subscriber and native decoder worker then processed the
same 42 requests, returned 39 matching frames/sequences, dropped no requests and
closed the worker. Its fixture SHA256 is
`82b5fb973e182779a6330650c72efbf2792c3655dc2267d474031f70080f6b2d`.
Four retained-frame tests cover generation identity, UInt64 sequence, fallback
notification, FD lifetime and invalid-layout/serialization rejection. Checked
V4L/ION primitives reuse the existing native boundary. Qualcomm event/flush
definitions are derived from the original `tools/replay/qcom_decoder.h`
(`71f5b01b9198b95a61d8511ad7fc5a4508ac3a5765936e03acb555cac274db77`),
with a build-time check against any system definitions. The host QUERYCAP error
test is not physical VIDC/ION validation.

Receipts and exact invocations are indexed by
`.omo/evidence/253-cluster-resume/live-media-checkpoint.json`. VIDC policy,
GPU NV12 packing/direct/PBO/DMA-BUF output, complete rendered/encoded cluster
composition, scheduling/locale and the normal managed entry remain in progress.
No device comparison or complete-runtime performance claim follows from these
host checks.

### Complete HUD composition checkpoint (2026-10-12)

The connected native HUD now matches 256 unchanged-source captures covering
eight requested screen modes, moving/parked/standby/camera states, light/dark
themes, Korean/English, metric/imperial and normal/swapped panels at 1920x480
and 1280x320. The same executable also matches 288 status/speed/route-video
captures, 192 ordered debug-plot captures and the 192-frame system/trip/debug
panel regression. All 928 RGBA comparisons are exact. Actual route-video and
composed Korean HUD PNGs were also inspected. Synthetic media patterns make
placement and image reuse observable; these captures are not vehicle frames.

The executable SHA256 is
`2a7e99779552f24c03392bfb9d7b1c45eb9b8c84fe13998ffefb0b2447ec736d`;
the original renderer remains
`fc3e52d6dacb71348b4b5d7fce26b05943fa61f33e58751b0935c9eb284adfdb`.
Receipts are under `.omo/evidence/253-cluster-{status,plots,hud-composition,panels}/render-full-first/`.
The ELF hash identifies the tested provider; worktree hashes recorded during
comparison are observations, not claims that subsequent edits were compiled
into that provider. The comparator now also rejects a provider changed during
capture. Strict Clippy passes for the library, HUD/replay tests and navigation
render fixture; all eight library tests, three HUD policy tests and the replay
clock test pass. Camera overlays, video output and ordinary managed startup
remain separate integration work.

The replay controls additionally match 64 exact RGBA captures, including
negative/overrun playback, minute rollover and ties-to-even time rounding,
pause labels, pitch/height tuning and clamped slider endpoints in both themes
and dimensions. The registered API is `draw_route_replay_controls` with explicit
playback/tuning values. Provider SHA256 is
`8784da25d5aefe1c0e093b26cdde600a1c3f868e46ad090b71a53785a36765e3`;
the receipt is `.omo/evidence/253-cluster-replay/render-first/result.json`.
This verifies the control surface, not the separate offline route-replay input.

### Native runtime monitor checkpoint (2026-10-12)

`monitors` now owns asynchronous network address and process-core sampling,
Git repository/HEAD discovery, remote status policy and bounded native Git
commands. The named workers return cached values, coalesce pending refreshes,
and join on close. Git workers drop realtime scheduling before running commands;
timeouts and cancellation terminate the owned command group before reaping its
leader. Tests use temporary Params/proc trees and local Git repositories.

Seven tests pass, including actual Git fetch/up-to-date/behind/deleted-branch
states, simultaneous stdout/stderr pressure, UTF-8 replacement, timeout cleanup
of a descendant, refresh deadlines, and repeated worker close. The unchanged
Python 3.12.14 source additionally matches 22 address cases, 320 ordered core
updates, two proc scans, four repository layouts and 68 Git status/command
sequences. The initial comparison exposed IPv4-mapped IPv6 display/filtering
differences; those now match the pinned Python source. A separate malformed
proc fixture verifies that negative thread-directory names are ignored.

Provider SHA256 is
`b55b76bd96d8655cf00e4298c9a1c5aea3164d08dd78926ea10610b9eb3ab28d`.
The source comparator is `rust/tools/cluster_monitors_compare.py`; receipts and
native test invocations are in `.omo/evidence/253-cluster-monitors/` under
`source-green/` and `source-fix.json`. These providers are ready for managed-entry
composition; this checkpoint does not establish normal cluster startup.

### Native command-line policy checkpoint (2026-10-12)

`runtime_args` implements the 78 original arguments, typed choices, defaults,
validation, exact-option presence tracking and abbreviation handling. The
managed wrapper must still explicitly select live input: the original parser's
literal default is random. Metric and imperial switches remain mutually
exclusive. Root-relative native library lookup and Python path/decimal parsing
are preserved; integers must fit the runtime's checked signed 64-bit fields.

The unchanged source parser matches all 658 comparison cases, including parsed
fields, float bits, validation policies, Unicode numbers, path normalization,
and four TICI/eGPU output-rate combinations per successful parse. Device live
USB output retains the fixed 10/5 FPS policy, including display commands.
Non-live H264 defaults and explicit rates retain the source rounding behavior.
Help is exposed as a successful usage request; exact argparse help typography
is outside this comparison. The native provider SHA256 is
`947ab85488af5aec8075f5d06dfd360e3238ecb7d5b1910d18635213da4f36b9`.
Receipts are in `.omo/evidence/253-cluster-args/source-green/result.json` and
`fourth.json`; the comparator is `rust/tools/cluster_args_compare.py`.
Four CLI tests and the seven monitor tests pass, and strict Clippy passes for
the current library, both test targets and the CLI fixture. These checks prepare
the entry-point composition; they do not establish complete managed startup.

### Simulator and controller-input policy checkpoint (2026-10-12)

The registered `simulator` module now provides the original synthetic vehicle
and deterministic random-input source. Ordered comparisons cover 3,120 manual
steps and 1,205 random steps, checking every UI field and internal state field.
Cases include both/held signals, completed lane changes and recentering, camera
lane observations, steering overrides, surround-view decay, capped time steps,
maximum speed and braking to rest. Numerical comparisons allow 2e-14 relative
and absolute error; discrete states and collection structure match exactly.
Provider SHA256 is
`fa6ddfd964db2db91531367020db61da30a14d8581082e6e35662694fe2bf796`;
receipt: `.omo/evidence/253-cluster-simulator/source-first/result.json`.

The original gamepad input policy additionally matches 480 ordered frames and
104 normalization cases, including warmup boundaries, controller versus generic
joystick scaling, missing axes/buttons, hats and status text. Its provider is
`fd10dd254f8c19c5a14c5999e3ac523494de55d9624720f019b63e3ada1b30a7`;
receipt: `.omo/evidence/253-cluster-gamepad/source-first/result.json`.
The comparison injects raw SDL samples and a clock into unchanged source methods.
Pygame's generic-axis divisor is independently checked against its
[2.6.1 implementation](https://github.com/pygame/pygame/blob/2.6.1/src_c/joystick.c#L348).
Three native policy tests pass. Native SDL device opening, event pumping and
lifecycle integration remain separate work; this is not a connected-controller
or full normal-startup result.
## Native route-log reader checkpoint (2026-10-13)

The route reader now handles raw, Zstandard and BZip2 Cereal files and preserves
parser history across files. Three owned synthetic logs exercise 69 frames
through the unchanged original `RouteLogParser.parse_file` and the native
parser. All UI fields, raw lane geometry and replay metadata match. Source
comparison also matches all 26 byte-decoding cases, including concatenated
streams, empty files, suffix/magic precedence, corruption and truncated input.
The source Zstandard stream reader returns the decoded prefix at EOF; Python
`bz2.decompress` accepts invalid trailing data only after a complete stream.
Eight observed differences in the first comparison drove the native decoder
adapter, while invalid first BZip2 streams and incomplete BZip2 members still
fail. Raw decoder error messages are retained; their platform wording is not
claimed identical. The final reader fixture SHA256 is
`7f589bd21aa9fe37eeaa281d0a7a4e0b5c094e34778dbc95693774f736aac7a2`.
The receipt is `253-cluster-route/log-codec-green/result.json`.

An extended live comparison exposed four cases where invalid inner model lanes
incorrectly discarded the previous valid center. Empty, reversed, coincident
and nonfinite samples now retain the prior valid inner coordinates as the
original does. All 69 live comparison steps pass with the fresh provider
`0d49e73f18fd52f12d1d248741fa5f6660560cf4e9cadead4f4a36a673352c53`.
The failing and corrected comparisons are `invalid-lanes-red` and
`invalid-lanes-green` under `253-cluster-route`.

The decoder uses existing Zstandard 0.13.3 and cached BZip2 0.6.1 with
libbz2-rs-sys 0.2.5. Existing locked versions were preserved; the latter two
packages retain their MIT/Apache-2.0 and bzip2-1.0.6 licensing. The native
preload process, raw/stable CAN presentation and video/timeline composition
remain separate unfinished parts of the route entrypoint.

## Replay frame and bounded monitor shutdown checkpoint (2026-10-13)

The raw corner display decoder and stable display tracker now match 476 source
queries and 2,880 ordered observations. The comparison covers both CAN address
groups, payload lengths, position/velocity smoothing, duplicate labels,
measurement-age rollover, stale expiry, backward timestamps, hit gates, sorting
and replay-only yaw/centering adjustments. Floating values use the existing
`2e-14` absolute/relative tolerance; discrete fields match exactly. The fixture
SHA256 is `8761ad9b271c8f44198bb84e90ac2d6cf69c1ce84bbd42f237759b6e5b4cbf81`,
with receipt `253-cluster-route/corner-source-first/result.json`. The focused
native test and scoped strict Clippy pass. These are display policies; they do
not add raw-CAN cut-in recomputation to the normal preload path, whose original
parser explicitly disables that feature.

A subsequent 600-frame full-Cereal comparison first recorded 276 differences
from missing CAN presentation integration in `253-cluster-route/can-red`.
The parser now decodes CCNC/ADRV display messages through the existing Rust DBC
reader, applies source bus/service/length filters, preserves detection priority
and blindspot/edge exclusions, and selects raw/stable display points before the
existing live-track fallback. All 600 complete frames now match across live,
raw and stable modes with front-only filtering both enabled and disabled.
The receipt is `253-cluster-route/can-first/result.json`, fixture SHA256
`d4782a55c55c706ee88ea270c67b0faafbe4ff1c9d7078792854260a7dd575cd`.
Scoped strict Clippy and three native parser/projection/tracker tests pass.
The complete replay entrypoint remains unfinished.

The native preload helper now retains parser history and the first car-frame
time across files, sorts normalized frames, returns per-file failures, and
restarts with fresh state. It runs as an owned native child over a private Unix
socket; the replay coordinator uses one active parse request. Source/native
comparison matches six responses containing 300 complete frames through empty,
valid, malformed and missing files plus restart. Error presence is compared;
both raw diagnostic messages remain in the receipt instead of equating Python
tracebacks with native errors. Idle shutdown took 0.0021 seconds and shutdown
during an actual FIFO read took 0.5041 seconds; both children were reaped.
Nonblocking receive after unexpected child exit returns no result, while
blocking receive reports the exit. The focused normalization/history test and
scoped strict Clippy pass. Receipt `253-cluster-route/preload-second/result.json`
binds fixture `f360ee326e66e1dca6e5d2c2d0e004c2a6583759357f12f45a80e2e0f44df71b`.
Timeline, video and normal entrypoint composition remain separate work.

The timeline index and overlay text policies subsequently passed their separate
source comparisons: 432 samples across 48 append/trim batches, and all 1,440
lines from 120 overlays. Timeline cases include duplicate timestamps, exact
file boundaries, gaps, empty chunks, nearest-file choice, minimum-two-chunk
retention and strict one-second trim boundaries. Overlay cases cover optional
values, signed zero, percentage rounding, Unicode labels and summary truncation.
The corresponding receipts are `253-cluster-route/timeline-first/result.json`
and `253-cluster-route/overlay-first/result.json`; their fixtures are
`01fd1612c186ab0d10a962c3e8f4515a413b9722687c263e065c50ac3c17fcec`
and `01fa735708d8b8fd8c2b070ca43a45002c112692d0140a8fd3dec66abfb8baf3`.
The native boundary test and final scoped strict Clippy pass. A subsequent
borrow-signature cleanup changes no output policy. The full replay Source
composition and real video lifecycle remain unfinished.

The native replay `Frame` keeps continuous values, recorded discrete state,
raw lane geometry and overlay metadata separately. Blending reuses the live
lane-rendering and animation policy, preserving the source's discrete model
lane-change progress and the midpoint choice for recorded radar/model data.
An unchanged source AST comparison exercises 720 frame pairs and 2,160 complete
UI states, including optional values, model/empty lanes, colors, edge clipping
and lane-change phases. All discrete values match; floating values match within
`2e-14` relative and absolute tolerance. The compiled fixture SHA256 is
`6138156fcb9eb828530c3a8f814c8c7438ba1c934d9400cf93a58435e8ef519e`;
the current receipt is `253-cluster-route/frames-first/result.json`.
This validates frame interpolation, not the still-in-progress log reader,
preload process, video reader or complete replay entrypoint.

Review found that the native monitor worker joined indefinitely while the
original `_AsyncRefreshWorker.close()` waits at most one second. A blocked
callback reproduced the failure before the fix. The worker now waits for its
completion channel for one second, then detaches an unfinished thread whose
owned state remains alive until its callback returns. Explicit close and Drop
both meet the bound; all eight monitor tests and the focused frame test pass.
The raw failure and corrected results are retained in
`253-cluster-simulator/monitor-close-red.log` and `close-frame-first-0.log`.

## SDL device and route-discovery checkpoint (2026-10-13)

The gamepad source now opens the external SDL2 library directly through the
small `openpilot-sdl2` FFI crate. Rust owns controller-first/joystick-fallback
selection, event pumping, raw input snapshots, warmup, command conversion and
resource lifetime. SDL2 remains an explicit external native dependency, loaded
only when gamepad input is selected; Pygame is not used by this native path.
An owned SDL virtual controller and generic joystick exercise different axis
scales, buttons, hat direction, warmup, status text, repeated close, closed reads
and restoration of the pre-existing device count. Both the narrow FFI test and
the cluster command integration test pass. Strict Clippy passes for SDL's
fixture feature and the cluster library plus the five focused tests.

Native route discovery preserves route/segment filters, sorting, missing route
prefix lookup, direct-file selection, symbolic-link resolution and adjacent
Zstandard/BZip2 filename preference. The unchanged source AST and native fixture
match 108 actual temporary-filesystem cases and 2,916 segment-name checks,
including Unicode decimal names. The fixture SHA256 is
`c3323fe32817d34d99febd9620a0c5aee22a9b7a998bdb0cbf1024547072e6ef`.
Private results are in `253-cluster-route/paths-first/result.json`; focused
tests/build/strict commands are in
`253-cluster-simulator/device-and-route-second.json`.
An initial test incorrectly expected `aaa---1` to have no segment; checking
Python's last `--` split established segment 1, and the expectation was fixed.
Path discovery alone does not establish log parsing or route playback. Segment
indices use checked signed 64-bit values, as do the native CLI integer fields.

### GPU output and hardware-first decoded media checkpoint (2026-10-13)

The Rust NV12 packer draws the existing capture texture through the original
Y/UV shaders, preserving portrait rotation, flip, filter/blend order, padding,
Venus layout and reused-buffer tails. All 96 original/native host GL cases
match every output byte, including the 480x1920 Venus allocation of 1,757,184
bytes. The first comparison exposed a cold-frame zero-luma defect from setting
custom blend factors after entering the blend mode; the checked graphics bridge
now follows the original order. Four decoded light/dark output images at both
source sizes were inspected. Provider SHA256:
`358be8a23805bd2c4c4a4003b14bcdf178cdbfe99f7bf8912d9834374ea186ff`.

Native Rust VIDC decoder policy now owns format/control setup, ION buffers,
queues, retained generations, port-change/flush handling, timeout and cleanup.
The unchanged C++ source and current native fixture match 24 controlled kernel
scenarios and ordered V4L/ION traces. Literal output matches 23; the remaining
QBUF failure preserves errno 22 and strerror with Rust's error formatting.
The comparison explicitly normalizes that formatting and excludes the global
source versus owner-local native ION-device FD lifetime from trace equality;
raw traces remain available. Vendor constants come from the pinned source
UAPI header, with compile-time checks against conflicting definitions. Current
fixture SHA256:
`dda489323c81a239e411e22c641cbc8a121b7322101107a00bc4b7fafcc2f937`.

The real Cereal media worker now selects the Rust VIDC owner first, retains an
owned frame FD, and falls back permanently to FFmpeg after an EGL-import failure.
An original-wrapper control matches the native IPC path's layout, token,
sequence, FD validity after owner close, fallback strides and visible YUV bytes.
Both explicit software selection and actual host QUERYCAP failure also process
42 owned IPC requests with 39 decoded frames matching the original software
pixel/stride oracle. The current hybrid fixture SHA256 is
`a9c3ef4041da6b9426abdac1b1221dea9182b94f19cb201c1bb27adb4e460c2f`.
Native-media library tests, two VIDC tests, four retained-frame tests, two NV12
tests and scoped strict Clippy pass. Receipts are indexed by
`.omo/evidence/253-cluster-resume/media-gpu-checkpoint.json`; reproducible
comparators are `cluster_nv12_compare.py`, `cluster_vidc_compare.py` and
`cluster_media_hardware_compare.py` under `rust/tools/`.

The VIDC control uses an owned host fake-kernel provider, not a vehicle or a
physical codec. Successful physical VIDC and external EGL pixels remain
unvalidated. The cluster VIDC encoder, asynchronous readback/direct DMA-BUF
pools and complete normal main/output/startup/upload composition remain in
progress; these checks are intermediate engineering evidence.

### Rust cluster VIDC encoder policy checkpoint (2026-10-13)

`h264::vidc` now owns the source cluster encoder's NV12 format/control policy,
Venus layout, seven input and six capture slots, acquisition/cancellation,
CPU/DMA-BUF cache synchronization, packet/header handling, timings and cleanup.
It reuses checked V4L/ION primitives in the existing `encoderd::native` boundary;
no project encoder C++ policy is loaded at runtime. A scoped acquired-input
callback supplies the mapped byte slice and borrowed FD for direct graphics
readback/import without a second input allocation. Source flag definitions are
read from the pinned original encoder and checked against conflicting UAPI values.

Two focused tests and strict Clippy for the native-media library, test and fixture
pass. The unchanged original C++ methods match 48 controlled host-kernel cases,
480 packet callbacks, queued-input hashes including padded tails, and ordered
V4L/ION traces. Cases cover portrait and small layouts, full/active/scoped/DMA-BUF
input, cancellation, all eight rate-control modes, slice/profile fallback, format
rejection, cleanup and maximum unsigned timestamps. Literal output matches 46;
the two QBUF/control errno failures are compared after explicit error-format
normalization. Source-global versus native owner-local ION-device lifetime is
excluded from trace equality, with raw traces retained.
Provider SHA256:
`79db9ab8af945df1112dfff1103b1f9fb2bf144dfcadbec129f6dee82d4adab5`.
Receipt: `.omo/evidence/253-cluster-resume/vidc-encoder-checkpoint.json`;
reproduction: `rust/tools/cluster_vidc_encoder_compare.py`. This proves the
project queue/control policy against an owned fake kernel; it does not prove
physical hardware encoding. Native USB sender/SPS compatibility, direct GPU
output and normal main/autorun composition remain in progress.

### Route source composition checkpoint (2026-10-13)

The Rust route source now composes the persistent parser helper, bounded timeline,
corner correction, overlay and camera stream reader. The unchanged original
`RouteReplaySource` and native source match all 18 samples across six real Cereal
log files, including sequential loading, early rewind/reset, end-of-route status
and looping after the original source has trimmed old chunks. The comparison
checks complete UI state, overlay text, status, duration, loaded file count,
log path/folder/offset and completion. These logs have no camera files; actual
video comparison is separate. Receipt:
`.omo/evidence/253-cluster-route/source-first/result.json`; native SHA256
`bd0b11af5e0ec2bc6c99a79a86758d371e5ff8e96501c6f791def641fdea04e2`.
This remains host composition evidence, not the full runtime delivery gate.

### Embedded navigation projection checkpoint (2026-10-13)

Standalone navigation input now reuses the native Carrot Navi HTTP/WebSocket
receiver, Cereal projection and cluster decode worker. The original
`NaviSimulatorSource` and native projection match all 34 complete UI states from
126 receiver operations: negotiation, speed/vehicle precedence, repeated updates,
PNG/JPEG media, cleared items, map age at/beyond 3000 ms, wall-clock reversal,
disconnect and a new session. The source comparison suppresses only embedded
socket startup, controls time/session entropy, and exercises the original
receiver and source methods. Live socket composition is a separate check.
Receipt: `.omo/evidence/253-cluster-route/navi-first/result.json`; native SHA256
`f46532cf3791cdd8d1122bb3fc875a95a1f80594c1beaa438bef0b21b12eb833`.
The disconnected projection test and focused strict Clippy pass. Two owned
embedded-listener tests verify health, occupied-port rejection and port release.
The imported managed-entry runtime tests verify inherited logging context,
typed failure diagnostics, interruption and standalone return in real child
processes. No vehicle connection or discovery broadcast was used in these checks.

The final navigation fixture (`7456ff9b406561548bafd624e9196241aa5924f6ea3fcfbda6cc8c2a3117758c`)
also passes the 34-state comparison and an actual loopback HTTP/WebSocket
composition run. Both original and native servers negotiate the complete
catalog, receive vehicle/speed JSON and PNG binary data, clear the image and
disconnect. Five selected protocol-derived UI snapshots match; raw states are
retained and naturally varying clocks, session tokens and simulated-world time
are excluded from this live comparison. Both processes exit and release their
ports (original 0.0198 s, native 0.00061 s observed close). Receipts:
`.omo/evidence/253-cluster-route/navi-final/result.json` and
`.omo/evidence/253-cluster-route/navi-server-final/result.json`.
Three embedded-server tests additionally cover an unresponsive upgraded peer;
an owned loopback UDP test verifies the discovery packet and prompt close.
The test target redirects discovery to its owned socket; no LAN broadcast or
device connection occurs. Focused tests, fixture build and strict Clippy are
recorded in `navi-third.json`. Full main/output/startup/upload composition remains
the delivery gate.

### Cluster sender, preferences and rendered output checkpoint (2026-10-13)

Rust SPS compatibility matches 954 original cases, including malformed packets,
crop/constraint/VUI handling and the source one-entry cache. Native access-unit
retention matches 5,588 original sequences / 46,308 ordered steps. Four actual
sender tests observe negotiated chunks, acknowledgement flags, retained units,
control dispatch, transfer failure and repeated close. Main preference readers
match 330 cases using the unchanged Cython Params binding and original reader
methods; missing and unavailable Params owners retain their distinct defaults.
An empty retained-frame EGL failure now sends the source fallback text after
an exact failing regression.

Four owned process scenarios now compose actual carState/deviceState IPC, full
cluster rendering, portrait RGBA or GPU NV12, encoder ownership, the existing
USB transport/controller and cleanup. All 96 live projected frames reach output.
Both software streams decode 23 frames from 24 submitted and reap their children;
the small stream matches original bytes. The portrait stream preserves exact
encoder arguments, decoded dimensions/count, controller commands and each
implementation's dump/wire bytes, but is not byte-identical across runs: two
unchanged original runs on the identical RGBA input also differ. Those raw
controls are retained rather than claiming portrait bitstream equality.
The two controlled VIDC scenarios match every mapped GPU input hash and frame
timestamp and observe render cancellation and repeated close; the kernel
provider is synthetic and does not establish physical encoded pixels.
The decoded portrait image visibly retains speed, vehicle, lanes and panels.

Receipt index: `.omo/evidence/253-cluster-resume/`
`sender-preferences-composition-checkpoint.json`. Current composition provider
SHA256: `e38b5c4b35e2e8411f1b9b955f2b65ff40f210fa700c80064e3ee9275f4649a0`.
Reproduction tools: `cluster_native_queue_compare.py`,
`cluster_preferences_compare.py` and `cluster_pipeline_compare.py`.
Strict retry after two exact-FPS assertion fixes, descriptor-driven startup,
locale, direct GPU output and complete main/autorun/startup/upload composition
remain in progress. These are intermediate host engineering receipts.

### Output startup and locale checkpoint (2026-10-13)

The thread-confined USB owner now reports opened display dimensions to the
caller before encoder configuration and stream negotiation. The software check
observes the ordered open/configure/stream calls, an actual 6x4 FFmpeg command,
801 output bytes and child reap. Native initialization failure on `/dev/null`
retains QUERYCAP errno 25 before any stream negotiation. Existing stream APIs
remain available. Nineteen focused tests, five fixture builds and scoped strict
Clippy pass. Locale matches all three original fallback controls and actual
same-PID re-execution propagates C.UTF-8 to all three environment keys and a
native child. The fresh integration provider also passes the four 96-frame
Cereal/render/encode/USB scenarios with the portrait source nondeterminism
limit recorded above. Receipt index:
`.omo/evidence/253-cluster-resume/startup-locale-checkpoint.json`.
Fresh composition SHA256:
`9d8631dd752bcbeeff30e7151b2fea86442bdf1d94a048bb91b632a80ee99041`;
locale SHA256:
`dc672e017b39a3bf7e17bf8396d6fde2a91f9e8bbf3fa399a14b4e2b673f47b6`.
Direct/PBO/DMA integration, native profiling and full main/autorun/startup/upload
composition remain in progress; this is not a device-test handoff.

### GLES output lifetimes (2026-10-13)

Owned host GPU targets exercise the unchanged Python and native direct/PBO
readback: all 15 queue/state observations and six complete buffers agree,
including FIFO order, three-slot saturation, resize discard, binding restoration
and untouched trailing bytes. Seventeen provider failure controls also agree
and finish without live buffers, fences or mappings. Receipts are
`.omo/evidence/253-cluster-renderer/gles-gpu-green/comparison.json` and
`gles-errors-first/comparison.json`; the successful GPU fixture SHA256 is
`39dc1e40243283abe0dce0df31843146e7c977a8f9b936062a7e8b5ae5519df0`.

The DMA pool corrects a demonstrated source cleanup defect. In
`cluster_gles_dmabuf.py` after successful EGLImage creation, `_clear_gl_errors`
raises before the import's `try/finally`; an uncleared GL error queue leaves the
duplicated FD and image outside the cache. The owned provider reproduces source
FD delta +1, one live image and zero destroys. Native RAII preserves the exact
error and binding state while producing FD delta 0, no live image and one
destroy, with the duplicate still open during image destruction. The full raw
source/native controls remain in
`.omo/evidence/253-cluster-renderer/dmabuf-first/comparison.json`; native fixture
SHA256 is `f9f82f448d957ca74e081b4e7818465160a1b63596ce0ea3be2e65dde8182f5e`.
The comparator records this correction explicitly instead of treating it as
source equality. This is host lifetime evidence; successful encoder DMA import
and physical AGNOS graphics behavior remain target validation gates.

## Main input and replay composition (2026-10-13)

The runtime input adapter now owns random/gamepad simulation, live subscriptions,
the embedded Navi receiver, route source/helpers and optional Navi overlay. Its
route path preserves replay rate, pause/seek ordering, parser option reloads,
numbered-log navigation, one-second end retries, recorded-sound cut-in pauses,
overlay/video visibility and separate-tool/window updates. Live `--live-no-can`
retains precedence over `--live-include-can`. Source profile samples use
milliseconds and owned close paths attempt both main and overlay cleanup.

The missing Input/Playback/front-only parser APIs were recorded as compiler REDs
before implementation. Focused Input/Playback unit tests and both route parser
tests pass. Actual unchanged `main.py` control branches and its original
`CutinAlertTracker` produce exactly the same 2,880 ordered clock states across
36 scenarios; `.omo/evidence/253-cluster-route/playback-first/result.json`
identifies native fixture
`5c17bcf14886ee386c71053acf8b729d3781b18c44a68117ba32fb029a320ba2`.

Parser reload sends the explicit front-only choice over the owned preload IPC
instead of mutating a process environment while Rust workers are running.
Both settings were tested against the opposite inherited environment, including
six actual Cereal files, backward seeks and preload restarts: all 36 complete UI
states agree with Python, with corner support true only when front-only is off.
The reports are `front-only-{0,1}/result.json` under the same evidence directory;
the native Source fixture is
`e7e36b6fe307b876a86f8c18c4595b6d0ed3fb448a13f74e9a5e7e8158b30796`.
No detection or lead-selection policy was changed. The same complete library,
route parser tests and both new fixtures pass strict Clippy; see
`runtime-first.json` for the disk guards, commands and results.

The shared Navi overlay function also matches all 864 full UI results across
source, limit, navigation-active and clock combinations, recorded in
`navi-overlay-first/result.json`. These tests establish composition policies and
owned source/helper behavior. Full installed-main startup, output lifecycle,
package closure and existing-log upload remain the delivery gate.

The two CLI string normalizers for screen mode and panel layout now preserve
all 35 original aliases, all 29 Python whitespace characters, Unicode decimal
digits and integer-limit behavior. The missing API compiler RED precedes the
implementation; the focused test, fixture build and strict library/fixture
Clippy pass. `config-normalizers-first/result.json` records all 305 input pairs
matching the original `cluster_config.py`, with native fixture
`0808f704eb1e57fee7d75b57f713f2696f56ea962596adac83875a7c6e3aa2c4`.
Separate replay tools are deferred until `Input::start`, after the main output
gate and before the renderer opens. Route-mode Navi overlay startup precedes
route loading as in the original; these connections remain part of the pending
actual-main startup checks.

The runtime output pattern control runs the original function bodies from
`main.py` and `cluster_h264_pipeline.py`. All 238 cases match: 38 RGBA buffers,
76 NV12 buffers including row/plane/trailing padding and odd half-quadrant
boundaries, and 124 bitrate results/errors including Unicode whitespace,
decimal digits, suffixes and rounding. Bitrate numeric equivalence is scoped
to the signed native C-int range. The fixture is
`cluster_output_patterns_fixture`; full bytes and results are retained in
`.omo/evidence/253-cluster-route/output-pattern-first/`, with native SHA256
`b53ee4ea86f6e0147958edb739dbf6cc58eb7c0b5103bf21fe2804a2b63e6df4`.
Its build and Python Ruff check pass. The first strict Rust check identified
three style diagnostics in the concurrently added output factory/backend;
the output owner's following strict batch owns their resolution. This control
does not establish USB or hardware encoder behavior.

The pure H264 diagnostic window and debug helpers now preserve the original
NAL accounting, queue/drop/send counters, interval and forced-window reset,
bitrate/timing formatting, byte prefixes, callback/V4L2 flag names, log cadence
and SPS summaries. Missing-API compiler failures preceded implementation.
The diagnostic control matches 1,006 packets and all 3,654 operations in
18 scenarios. The debug control matches 1,510 packets at four summary limits,
36 flag combinations and 604 log-cadence decisions. The shared VUI reader now
returns its existing parsed fields for diagnostics; all 954 original SPS
mutation cases still match after that change. Four focused Rust tests and
strict library/fixture checks pass, including the earlier output-pattern fixture.
Results are under `.omo/evidence/253-cluster-route/h264-diagnostics/`:
`source-final/result.json`, `debug-source-final/result.json`,
`sps-regression/result.json` and `debug-third.json`. Final diagnostic/debug
fixture hashes are `7bd8b13a79ec95491445c5196d35e71d83ea3312a3be409a332ef9d9b15e52c7`
and `537146f92c9765187e235d1eb22785156a247f23e81df63aa5e05c8d76531bbd`.
Runtime hook and physical output acceptance remain separate from these pure controls.


### 2026-10-13 cluster execution composition checkpoint

`runtime::run` and both executable entrypoints now compose project-owned input,
renderer, Params polling, monitor, scheduling, USB codec and cleanup owners.
`openpilot-cluster-autorun --root <checkout>` uses the existing normal autorun
policy and dispatches `openpilot-cluster` helpers before managed setup. The
process/daemon identity is `carrot_cluster`. This is an integration checkpoint;
final normal candidate packaging/startup/upload acceptance remains pending.

The current owned live-IPC receipt is
`.omo/evidence/253-cluster-resume/main-live-second/result.json`: six scenarios
produce 14 software H264, 14 fake-kernel native H264, 22 PNG, 22 synchronous
JPEG and 22 asynchronous JPEG runtime frames. The early HUD gate produces zero
frames and no H264 stream-start command. Each successful live case observes
real Cereal car/device publications, `onroad=true`, and 64.8 km/h. Software H264
dump and USB bytes match and decode to 13 delayed 64×96 frames; native input
maps are queued 14 times. The full PNG capture is a rendered 480×1920 portrait
upload of the 1920×480 cluster scene. Every scenario closes the owned USB handle
and returns `ClusterHudConnected` to zero. All observations are host controls;
fake native packets are not evidence of physical encoder output.

Standalone automatic H264 fallback first reproduced two USB opens where the
original Python runtime opened once. The retained worker owner now preserves
one display across failed native initialization and software fallback. The
actual original and native controls both record one open and final disconnect,
with RED/Source/GREEN receipts in `main-auto-red`, `main-auto-source`, and
`main-retained-green`. The explicit autorun encoder attempts retain their
existing transport ownership.

The direct NV12 output seam has 384/384 source byte comparisons: staged,
direct destination, genuine PBO and ordinary external-FBO modes. The runtime
reporter matches 120 source cases / 7,200 ordered operations. Exact invocation,
provider hashes and limits are indexed in `main-integration-checkpoint.json`.
Physical target DMA import and device performance remain unproved. H264
observability, final entry/helper/interrupt/Params controls, and the complete
candidate package/normal startup/upload receipts remain outstanding.

### Final cluster host composition receipt (2026-10-13)

The cluster executable now owns the complete reachable main/autorun composition:
input selection, live Cereal/Params projection, simulator/gamepad, standalone
Navi and route helpers, renderer, monitors, scheduling, PNG/JPEG/asynchronous
JPEG/software H264/native VIDC output, profiling and ordered cleanup. The normal
entry is `openpilot-cluster-autorun --root <checkout>` with manager identity
`carrot_cluster`; it supplies `--input live` rather than changing the standalone
CLI's original random default. `openpilot-cluster --root <checkout>` also owns
the preload and replay-tools helper dispatches before managed setup. Neither
entry invokes Python. Fixed TICI USB rates remain 10 FPS, or 5 with UsbGpuActive,
and the approved core7 SCHED_OTHER/nice19 and offroad little-core policy remain.

The final observed live composition is
`.omo/evidence/253-cluster-resume/main-observability-third/result.json`.
Six actual owned Cereal/Params scenarios produce 94 frames: 14 software H264,
14 fake-kernel native H264, 22 PNG, 22 synchronous JPEG and 22 asynchronous
JPEG; the early output gate produces zero. The software dump equals the
captured command-121 USB stream and decodes to 13 delayed 64x96 frames. Native
input maps are queued 14 times. PNG captures upload the full 480x1920 portrait
scene; JPEG paths upload 64x96 images. Each case observes the actual stage
samples, closes USB and restores ClusterHudConnected to zero. Per-case
`invocation.json`, `report.json`, wire bytes and image/decode artifacts retain
the precise commands and observables. The runtime fixture SHA256 is
`2bb9fd96d979b92661103d503e3090eb369089112bbf9e33c9dfcb062d4264bd`.

H264 instrumentation is connected to real read/callback/packet/send boundaries,
including native input acquisition, render/copy, queue and encoder timings.
The comparator executes the unchanged source debug methods on actual packet
boundaries: 40 software debug lines across 13 packets and 57 native debug lines
across 14 packets agree, normalizing elapsed time only. Its source SHA256 is
`d53b3339a560038ba53a446c528ee22c5438c968d342eaa7a826f4c3396b61d9`;
results and original lines are in `main-observability-third/debug-result.json`.
The actual media worker overflow/keyframe/epoch/resize/decode-failure control
also matches all 15 original trace events, 63 drops and recovered frames;
`media-worker-result.json` names the controlled decoder and both invocations.

The normal executable receipt `normal-entry-final/result.json` covers six
scenarios: disabled startup, output-gate wait, brightness/encoder/orientation
changes with two same-PID restarts and reaped software children, TERM/INT
cleanup followed by original retry/default-signal behavior, and automatic
software-to-JPEG fallback. `helper-entry-final/result.json` observes real
preload and separate GUI helper commands, reaps every owned child, and checks
the standalone Navi HTTP response and released port. Frozen main/autorun
SHA256 values are respectively
`b3c4203e763084a425632fcd6b73315d42e85b67d7d0b22c3e88f4971da22ff5`
and `a6e1894bf685e29ea7cf9575ec0d09ff87de5d282c72157aaa5b3fdac7981185`.
The sibling process-child SHA256 is
`6bf47b5a221247632bf8cf195609f04886e5bd4bc0c514993362626666663c91`.

Managed outer SIGINT first reproduced termination by signal during retry,
output-gate and GPU waits. The corrected managed scope preserves inner retry
semantics and source launcher behavior: all three final real-process cases
exit zero and publish one actual logging IPC warning, "child carrot_cluster
got SIGINT", with inherited session and corrected Rust/source context.
`managed-entry-final/result.json` retains the decoded messages. Separate
TERM/INT controls confirm default signals return after two closed inner scopes
(`signal-defaults-final.json`).

Whole-crate formatting was applied after those frozen executable receipts.
The only subsequent production extraction separates private system-panel draw
sections without changing arithmetic or order. A fresh native renderer fixture
(`db14f2aaf5cfdfd8e23248e541d6ff611dc3121657cea4863edf61231371bcb3`)
matches all 80 original light/dark, 1920x480/1280x320 PNG images byte-for-byte
and all 16 expected error controls; see
`system-panel-formatted-final/comparison.json`, replayed after final rustfmt.
Current source identities and
the final build/test/strict/format commands are indexed by
`cluster-runtime-final.json` in the same evidence directory.

The final formatted tree passes `cargo test -p openpilot-cluster --features
native-media --lib --tests`: 147 tests across 59 top-level executables, with
zero failures or ignored tests. Full `--all-targets` Clippy with `-D warnings`,
whole-cluster `cargo fmt --check` and `git diff --check` pass. The first final
suite replay exposed a test-only `/proc` argv visibility race; two of five
isolated original test runs captured an empty command line. The corrected test
waits for nonempty argv before asserting exact 6x4 geometry; 120 consecutive
real FFmpeg startups retain ordered startup, 801 output bytes and child reap.
The complete suite and strict checks were rerun after that correction. These
are recorded by `cluster-final-all-tests-green`, `cluster-final-all-targets-green`,
`cluster-final-format-diff` and `output-startup-race-{red,green}` receipts.

External native dependencies remain explicit: raylib/EGL/GLES, FFmpeg/libx264,
libyuv, libusb, JPEG/TurboJPEG, SDL2/SDL2_ttf/Tcl/Tk and kernel V4L2/ION. The
old custom H264 library option remains parsed for CLI compatibility while the
project-owned encoder policy is Rust; it does not load that removed C++ policy
provider. Rust has no Python GC to freeze. The startup banner is informational
and is not claimed byte-identical to Python. Hardware/fake-provider results
do not establish physical VIDC encoded pixels, target DMA import or device
performance. Package #269 must still complete its unified installed normal
startup and existing upload evidence before any user device comparison.
No device/NAS connection, deployment, CPU saving claim or settings change is
part of this receipt. Issue #253 and independent defect #271 remain open for
their acceptance gates.
