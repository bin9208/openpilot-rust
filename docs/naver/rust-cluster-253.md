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
Metadata-only discovery and configuration/endpoint APIs are the next additive
boundary; no control GET_DESCRIPTOR requests are substituted for cached metadata.

The registered process, actual live Params/IPC, USB I/O/recovery, rendering,
codec/GLES lifetime, nondefault inputs, complete entrypoint, host/ARM CI and AGNOS
packaging remain open. No C3X, NAS, physical USB display or vehicle is contacted.
Full normal startup and existing log upload must be composed before the user's
first device comparison.
