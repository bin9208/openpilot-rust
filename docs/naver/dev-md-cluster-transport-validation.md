# #338 cluster S3 transport and display scheduling

Assigned after planner/radar gates by the parent. Selected source is `ajouatom/openpilot` `e8e152dbcfd9574a1791ed12026d1880e87689b3`, common ancestor `43203371004e035bdb70a00a8dad29a4b657c6c3`. Work stays on dev-md. No S3, vehicle or NAS access/deployment is authorized.

## Source map and implementation sequence

| Source group | Native target | Required source evidence |
| --- | --- | --- |
| `carrot/cluster/cluster_wifi_protocol.py` | `cluster-wifi::{config,protocol,discovery,tcp}` | exact HMAC/header vectors; malformed/tampered/oversized/truncated frames; config type/range/permissions/atomic replacement; loopback challenge/discovery and reconnect |
| `carrot/cluster/cluster_wifi_display.py` | native display transport adapter using existing TURZX packet builders/JPEG pipeline | three-frame in-flight bound, ordered ACK checking, command drains, disconnect cleanup, late open after renderer and registration detection |
| `carrot/cluster/main.py`, `cluster_autorun.py`, rate/config paths | transport selection, lifecycle and output-rate owner | wired eGPU cap remains5 FPS; Wi-Fi eGPU may10 FPS; preserve source setting/registration precedence and renderer lifecycle |
| `tools/esp32s3_display/provision.py` | native provisioning CLI using existing cached serialport/crypto facilities | mocked nmcli/UART sequence, no password echo/persistence, byte-length/identity/version/storage confirmation, import whitelist |
| `tools/esp32s3_display/firmware/` | preserve exact native ESP-IDF firmware source boundary | source fingerprint/build configuration; firmware/toolchain build evidence where available; no flashing or actual-device interoperability claim |
| `common/display_scheduling.py`, `ui/ui.py`, cluster launch/autorun | shared ui-application Scheduler and native cluster callers | complete platform syscall order/state/error comparison; worker/child sweeps, online-core race fallback, offroad nice restoration, strict SCHED_OTHER bootstrap |

Implementation will follow the source state machine with test-first source fixtures and bounded host/ARM builds. Existing cached hmac/sha2/serialport dependencies are available; JPEG encoding and USB packet construction remain native existing facilities. Wi-Fi secrets only cross the explicit provisioning UART path; sender configuration persists the source-whitelisted version/device/key/fps/quality fields with atomic mode0600 replacement. No independent protocol or authentication redesign is intended.

Source scheduling differs materially from older memory: cluster uses core4; C3/C3X UI (`tici`/`tizi`) uses online cores6/7 plus little cores, while C4 keeps core6. All onroad display workers retain SCHED_OTHER/nice19; offroad falls back to little cores and attempts nice0 restoration. Include-little mode remains low priority even when both big cores are offline. Shared UI scheduling edits must be coordinated with the parent before starting; root owns other UI/startup work.

## Current status

Native S3 protocol, UDP/TCP, provisioning and selected cluster transport/rate/supervisor wiring are implemented with source fixtures and host/ARM gates below. Final display scheduling is wired and880 original-source sweeps pass on both architectures. Exact native ESP-IDF firmware links with the pinned SDK. Remaining engineering work is the repeatable native firmware build caller, additional full renderer/output startup/reconnect regression and provisioning password-prompt regression; physical display/vehicle acceptance stays user-owned. The following chronological entries retain earlier RED/draft states and are superseded by later receipts. Final source-matched replay packaging still awaits parent integration freeze.

## Protocol implementation boundary (current work)

Classical signal acquisition now has full HOST/actual-ARM owner, active/moving
fault and record-method evidence. Independent S3 work begins with the final
protocol file openpilot/selfdrive/carrot/cluster/cluster_wifi_protocol.py;
QCOM import/provider and exceptional signal runtime construction remain open.
A small native cluster-wifi crate will own configuration, authenticated
framing/session/discovery and bounded TCP IO. This keeps focused protocol tests
independent of the large GUI dependency tree; cluster integration will consume
its typed interface later. Shared UI scheduling remains untouched until root
coordination. Protocol source final constants:UDP/TCP3333, six big-endianu32
header fields,1MiB payload maximum,512-byte minimum request,512-byte maximum
ACK response, HMAC-SHA256 and source nonce/session message domains. ACK sequence
ordering is deliberately the display adapter's responsibility, as in source.

First source fixtures cover exact bytes/HMAC, fragmented reads and malformed
ACKs, config Python type/range/hex behavior and atomic0600 replacement/whitelist.
Then real loopback authentication/discovery, display queue/reconnect, native
UART provisioning and firmware source/toolchain receipts follow. No S3 device
or Wi-Fi setting will be contacted or changed by these tests.

Final protocol source is now retained with exact SHA256/Git blob provenance in
rust/upstream/cluster-wifi-source.json. Temporary reference generator
rust/tools/check_cluster_wifi_protocol.py captures140 original-source cases
with no real network/credentials: config type/hex/range behavior, payload
limits and sequence overflow, exact header/HMAC/wire hashes, fragmented ACK
read calls and early-error consumption, nonce/session key derivation and
constructor failure cleanup. Original source class/functions execute unchanged
against isolated tempfile paths and inert socket objects. Receipt:
.analysis/scratch/2026-10-15-dev-md-planner/cluster-wifi-source2/receipt.json.
Native comparison, discovery, atomic configuration IO, display integration,
provisioning and target interoperability remain pending; this source capture
is not a native transport implementation.

Native protocol/config core is now written in an unregistered lightweight
cluster-wifi crate, with source140-case fixtures and an example trace. It has
separate typed registration, Python-compatible hex/config validation, atomic
whitelist0600 replacement, HMAC/session handshake and bounded send/ACK IO,
including source read/error order. The generic socket trait makes source
fragmentation/cleanup tests independent of GUI/device code. This draft is not
yet compiled or selected; workspace registration waits the coordinated Cargo
slot. Actual TCP/UDP adapters, discovery, display integration and provisioning
remain pending.

2026-10-15 native protocol gate supersedes the preceding uncompiled-draft status.
`openpilot-cluster-wifi` is registered in the workspace. The expanded original
final-source corpus has 149 configuration, atomic-save, framing, ACK and session
cases; host and actual ARM source fixtures are byte-identical. All 149 native
comparisons and two real loopback TCP tests pass on host and ARM/QEMU. TCP tests
cover fragmented authentication/data replies and socket closure after identity
or server-MAC failure. Strict all-target Clippy passes. Evidence is retained in
`cluster-wifi-host1.log`, `cluster-wifi-source-arm1.log`,
`cluster-wifi-tcp-arm1.log`, and `cluster-wifi-cache-cli-clippy2.log` under the
planner scratch directory. No display transport has been selected yet.

UDP discovery is now a separate native draft with an unbound IPv4 datagram
adapter and explicit nonce/clock/socket boundaries. Ten additional unchanged
source cases exercise authenticated replies after malformed packets, zero and
negative deadlines, receive timeout, and nonce/socket/send/receive failures,
including exact operation order and cleanup. Independently executed host and
actual ARM source fixtures are byte-identical (`s3-discovery-source1` and
`s3-discovery-actual-arm-source1`). Native fixture comparisons and the real UDP
loopback test are written but await the coordinated Cargo slot; they are not
reported as passing. Display worker integration, provisioning, firmware/toolchain
and scheduling remain pending.

The unchanged final `cluster_wifi_display.py` is additionally pinned in
`rust/upstream/cluster-wifi-source.json`. Seven inert source scenarios cover
21 transactions: three-frame backpressure, ordered command/no-reply draining,
sequence mismatch, send/result failure and closed transport. Host and actual
ARM source fixtures are byte-identical (`s3-display-source1` and
`s3-display-actual-arm-source1`). The native `Queue` draft preserves pending
sequence state on failure, reports disconnect before closing the connection,
and uses exclusive mutable access to serialize the complete transaction.
Its source-comparison test is written but uncompiled. These tests do not yet
cover display open/brightness/close, renderer startup, provisioning or firmware.

The UDP and display FIFO draft gates subsequently passed: full native crate
host and actual ARM/QEMU tests, plus strict all-target Clippy, are green in
`s3-udp-display-host1.log`, `s3-udp-display-arm1.log` and
`s3-udp-display-clippy1.log`. The actual localhost UDP test verifies request
HMAC and ignores a tampered reply before accepting the correct one. The full
149-case prior corpus and two real TCP tests also pass in this batch.

Provisioning source coverage now includes 47 unchanged-function cases for
UTF-8 SSID/password byte boundaries, NUL rejection, UART prefix/JSON handling,
error replies and exact deadline/read ordering. Independent host and actual
ARM fixtures are byte-identical (`s3-provision-source1` and
`s3-provision-actual-arm-source1`). Native validation/request/registration and
serial adapter code is in progress and uncompiled; the full registration
sequence, nmcli/password prompt, actual CLI, import and runtime wiring remain
pending. All fixture credentials are synthetic.

Provisioning drafts now include the native `cluster-wifi-provision` command
(feature `provision-cli`), bounded native subprocess capture for nmcli,
terminal echo restoration, UART115200/250ms reads and atomic registration import.
The 11 original full registration-sequence and eight active-profile-selection
fixtures also match independently executed host/ARM source captures. Tests now
cover those policy cases, an actual private PTY registration exchange and a real
CLI atomic import with no Python executable on PATH. These newly written
provisioning tests have not yet been compiled or run. The serial lifecycle
adapter remains separately testable; source policy fixtures explicitly inject
its open/reset/close boundary and do not claim to verify hardware line signals.

Provisioning gate is now green on host and actual ARM/QEMU:
`s3-provision-host1.log`, `s3-provision-arm1.log`, and strict all-target
`s3-provision-clippy1.log`. This includes 47 credential/UART policy cases,
11 full registration policy sequences, eight active nmcli-profile cases,
a real private-PTY fragmented/noisy HELLO/CONFIG/PAIR exchange with exact
Unicode JSON bytes, and actual CLI import/atomic replacement without Python
on PATH. Serial line-signal semantics remain an external hardware boundary;
the private PTY verifies native byte IO/lifecycle, not electrical signals.

Selected native cluster wiring is now present: registration bypasses desktop
USB-only fallback, forces async JPEG and registration quality, applies source
Wi-Fi FPS/default15/live eGPU cap10, and prepares renderer resources at
1920x462 before starting the network output worker. Existing native TURZX packet
builders, renderer/orientation and JPEG queue are reused. The source's two S3
selection blocks plus actual `ClusterUsbFpsReader.read` produce 120 unchanged
source cases, byte-identical on independently executed host/ARM references;
native host and ARM/QEMU tests pass (`s3-selection-host1.log`,
`s3-selection-arm1.log`). This is scoped selection coverage, not full
`main.py` closure. Strict cluster-library Clippy passed before the final small
`apply_wifi` method extraction; final all-target cluster lint and full display
open/close/reconnect/startup/late-registration gates remain pending.

The pinned external ESP-IDF5.4 SDK resolves to
`67c1de1eebe095d554d281952fde63c16ee2dca0`. Its official tool manifest drives
checksummed downloads under `/home/fma1/rust/cache/esp-idf-tools-v5.4`.
The system Python lacked ensurepip; a seeded isolated uv environment repaired
only that external SDK cache, after which the official constrained requirements
install passed. Firmware CMake configure passed against an unchanged scratch
copy of the 12 source-pinned native firmware files. A two-job local firmware
build is running; no link or target execution result is claimed yet.
SDK setup reference: https://docs.espressif.com/projects/esp-idf/en/v5.4/esp32s3/get-started/linux-macos-setup.html .

Final lifecycle and supervisor gate:

- Eight original `S3WifiDisplay.open/close` cases now compare complete encrypted
  TURZX command bytes, print/connection effects, FIFO/sequence failures, send and
  receive failures, cleanup and both inherited200ms no-ACK command gaps. Source
  host/actual ARM fixtures are byte-identical; native host and ARM/QEMU pass in
  `s3-lifecycle-autorun-host1.log` (library filter) and `s3-lifecycle-arm1.log`.
  Static linking replaces only the Python protocol-helper import boundary;
  original vendor encryption and original `_send_command` execute in the oracle.
- Actual native supervisor tests cover registration bypass of desktop USB
  fallback, registration created during the USB wait, and invalid registration
  propagation before any USB scan/child. Both tests pass on host and ARM/QEMU
  (`s3-autorun-host2.log`, `s3-autorun-arm1.log`). The production native platform
  reads the same typed registration loader and checks it at the original100ms
  HUD-check boundary. The managed interrupt used to bound tests is retained
  runtime infrastructure, not a source scheduling threshold change.
- `s3-runtime-scheduling-clippy2.log` passes strict all-target cluster Clippy.

Final display scheduling gate:

`rust/upstream/display-scheduling-source.json` pins the complete final helper,
UI entry and supervisor source. The final helper delta from the common ancestor
is fully mapped to `ui-application/src/scheduling/{mod,native}.rs`: online extra
cores, optional little-core union, low-priority determination, worker/child
sweeps, policy-before-affinity order, hotplug fallback and permission handling.
The native UI entry calls `Scheduler::for_ui` with the actual device type:
tici/tizi use little+6+7 onroad, other UI hardware keeps6. Cluster renderer and
supervisor select4. Existing SCHED_OTHER/nice19 and offroad little-core restoration
remain enforced.

The old native C3 trace is RED at sweep10: unaffected worker2 was pinned to6
instead of original source little+6+7 (`display-final-c3-red1.json`). Expanded
final original-source corpus has 11 configurations/880 sweeps, including actual
unchanged `ui.py` scheduler construction for tici/tizi/mici/PC/unknown, disabled
mode, duplicate extra cores, per-core online transitions, child sweeps, exited
workers and permission failures. Native host and ARM/QEMU ordered effects and
state are exact (`display-final-ui-{host,arm}-green2/result.json`); independently
executed actual ARM source traces are byte-identical to every compared reference
(`display-final-ui-actual-arm-source2`). The native own-process SCHED_OTHER
readback also passes. `display-final-ui-clippy1.log` passes the scheduler example
lint. This is host/emulated policy evidence, not device placement/performance
acceptance. Other portions of UI `ui.py` and cluster `main.py` are separate owners.

Firmware link completed with the exact final source and pinned SDK/compiler:
app875,344B (`0xd5b50`) fits its3MiB partition with72% free. ELF, app/bootloader/
partition binary hashes and compiler/tools-manifest SHA256 are recorded in
`rust/upstream/cluster-wifi-firmware.json`. Build log is
`s3-firmware-build1.log`. This establishes a local Xtensa firmware link, not ARM
firmware execution or hardware interoperability. The repeatable native caller is verified below; no flashing occurred.

## Final source-path coverage ledger

All paths below are selected at `e8e152dbcfd9574a1791ed12026d1880e87689b3`.
Their exact source blobs are in the two source manifests and firmware manifest.
No native commit is invented for the current uncommitted shared checkout.

| Exact source path | Native implementation | Verified evidence / remaining scope |
| --- | --- | --- |
| `openpilot/common/display_scheduling.py` (last source change `48f1649e787b2dee6c3f9af75ad4fb60c6a49720`) | `ui-application/src/scheduling/{mod,native}.rs` | Final constructor/update delta closed;880 exact ordered sweeps HOST/ARM, source C3 RED then repair, native SCHED_OTHER readback; unchanged native sysfs/proc helpers retained. |
| `openpilot/selfdrive/ui/ui.py` | `ui-application/src/runtime/mod.rs` + `Scheduler::for_ui` | Scheduling construction closed by actual source statements for all five device labels and disabled mode; rest of UI entry remains root-owned. |
| `openpilot/selfdrive/carrot/cluster/cluster_wifi_protocol.py` (origin `5a3baf7c7404f36cabc46720768b74870410a4ff`) | `cluster-wifi/src/{config,protocol,discovery,tcp}.rs` |149+10 source cases HOST/ARM, real TCP/UDP loopback; production S3 composes source default discovery then authenticated connection. No physical S3 interoperability claim. |
| `openpilot/selfdrive/carrot/cluster/cluster_wifi_display.py` | `cluster-wifi/src/queue.rs`, `cluster/src/usb/{wifi,display,io,frame,settings}.rs` |21 FIFO transactions +8 complete open/close source cases HOST/ARM, exact encrypted commands, failures/cleanup/delays. Full renderer/output startup regression still pending. |
| `openpilot/selfdrive/carrot/cluster/main.py` | `cluster/src/runtime/{entry,poll,output}`, `runtime_args` |120 source S3 selection cases HOST/ARM, renderer-before-network initialization wired; only S3 selection/lifecycle/rate and cluster core4 are owned here, remaining main behavior stays separate. |
| `openpilot/selfdrive/carrot/cluster_autorun.py` | `cluster/src/autorun/{controller,native,wait}.rs` | Native existing/late registration and invalid-file tests HOST/ARM; native supervisor selects core4; other existing supervisor definitions retained. |
| `tools/esp32s3_display/provision.py` | `cluster-wifi/src/provision.rs`, `src/bin/provision.rs` |47+11+8 source cases and real UART/privatePTY + real import CLI HOST/ARM; private TTY password no-echo/restoration and actual UART registration tests also pass HOST/ARM. |
| Each of12 exact paths under `tools/esp32s3_display/firmware/` listed in `rust/upstream/cluster-wifi-firmware.json` | Same native files preserved byte-exact | Actual ESP-IDF5.4/Xtensa link passes; last firmware source commit `0e647cd2d49a8b607583da076deb2f0837779374`; native repeatable build caller passes; physical target execution user-owned. |

## Native firmware caller and private terminal gate (2026-10-16)

`cluster-wifi-firmware` verifies the twelve source hashes, ESP-IDF commit and
tracked-source cleanliness, tool manifest and installed Xtensa compiler hash
before copying into its owned output. It checks the disk floor before copying,
configuring and building, runs CMake/Ninja with two jobs, and records output
hashes. The actual native CLI completed all1,042 build steps using the pinned
SDK/compiler above; app size remains875,344 bytes with72% partition free.
`s3-firmware-native1.log` and its `native-s3-build.json` retain this run's hashes;
path/time-dependent ELF/app hashes differ from the earlier manual build, so no
reproducible-binary claim is made. Unmarked nonempty output and wrong-generation
markers are rejected without modifying the sentinel files or creating sources
(`s3-firmware-negative1.json`). No flashing or physical device access occurred.

The real CLI private-TTY regression checks ECHO is disabled while collecting
the password, restores it afterward, does not echo or persist the password, and
completes HELLO/CONFIG/PAIR over a separate private pseudo-terminal. Together
with atomic import, both integration tests pass on host and actual ARM/QEMU
(`s3-firmware-tty-{host,arm}1.log`). All-target strict Clippy with both CLI
features passes (`s3-firmware-tty-clippy1.log`). Full renderer/output startup and
reconnect regression remains separate from these verified transport/lifecycle
and native firmware-build gates.

## #338 corrected scope and refreshed source (2026-10-16)

The user's corrected scope is changed runtime behavior only; development-tool,
compiler, provisioning and broad validation work belongs to a separate session.
Those existing files are preserved. Historical conversion gates above do not
expand this task. The parent owns the central source pin/inventory.

Refreshed source is `8e7eec049241b6ba384e9197aacdfb01f7d88ed3`.
The exact `e8e152d..8e7eec0` cluster runtime delta maps as follows:

| Changed source | Focused native change | Evidence/status |
| --- | --- | --- |
| `cluster/cluster_wifi_display.py` | `cluster/src/usb/wifi.rs`: sync then200ms before brightness, zero-brightness ACK, confirmed sync/off ACK shutdown |20 actual-source inert-channel cases captured in `cluster/tests/data/wifi_lifecycle.json`, including zero/nonzero brightness and connect/send/result/sequence failures; native execution pending coordinated slot |
| `cluster_autorun.py` | `autorun/usb.rs` chooses registered S3 for0092 dim before USB discovery; `autorun/entry.rs` and controller shorten only known S3 wrapper+IO failures | focused native tests/compile pending |
| `cluster/cluster_profile.py`, `cluster/main.py` GC restore | native equivalence, no GC emulation | `jpeg_async::Pipeline` owns its worker JoinHandle; worker owns output locally; Shared owns pending frames/error values and no Pipeline/output back-reference. `worker::run` stores an `Arc<io::Error>`, not a Python traceback retaining frozen frame locals. `close` signals and joins; Drop attempts close. Therefore the added Python frozen traceback-cycle/unfreeze fix has no corresponding ownership cycle in this Rust runtime. No memory/performance claim is inferred. |
| `tools/esp32s3_display/firmware/main/usb_bridge.c` | exact selected external native source refreshed | byte copy from8e7eec0; no new SDK/compiler/provisioning work or flashing |

Remaining old-range added cluster display behavior is now implemented but not yet
compiled: state/live one-second external-compute active badge, source eGPU
priority/dynamic width/center474; retained VisionIPC producer-ID check; live-road
signal candidate overlay. Pure display continuity/reader lives in
`ui-application/src/signal_display.rs`, reusing the existing UI dependency instead
of linking signal inference/OpenCV into cluster. Two focused continuity/freshness
regressions are written. The overlay never feeds a planner/control input.
`rustfmt` parsed the edited modules and scoped `git diff --check` passes; neither
is described as a build or behavioral test. No Cargo was run during the scope
audit. Host focused cases and ARM build remain pending coordinated ownership.

Focused closure receipt after the scope correction: all20 refreshed-source S3
lifecycle cases pass on host and actual ARM/QEMU; the retry-log regression first
failed (transparent error wrappers hid the IO cause), then passed after explicit
native wrapper traversal, including the production runtime/IO nesting. The two
pure signal-display continuity/freshness tests pass on host and ARM/QEMU. Only
these four focused test functions were executed. ARM `openpilot-cluster` and
`openpilot-cluster-autorun` link successfully. Shared cache free space was286GiB.
No broad regressions, firmware build, provisioning, device or NAS operation was
performed in this corrected-scope gate. The parent received Cargo ownership
handoff to the model agent after linking.

Final focused source review also preserves the Python ordered dictionary's
previous display order when tracks subsequently share a timestamp, with a third
regression. The retry regression covers authentication/protocol failures carried
inside native IO wrappers: these keep ordinary failure reporting and are not
misclassified as source OSError transport disconnects. All three display tests,
the retry regression and20 S3 lifecycle cases pass in the final host batch; final
ARM relink/execution follows this receipt.

Final corrected-scope receipt: the three display regressions, source20 S3 cases
and retry-wrapper regression pass under actual ARM/QEMU as well as host.
`openpilot-cluster` and `openpilot-cluster-autorun` ARM binaries were relinked
successfully afterward. Selected firmware `CMakeLists.txt` also now exactly
matches8e7eec0, including `PROJECT_VER "1.1.0"`; latest firmware source was copied,
not rebuilt or flashed in this corrected-scope gate. Cargo was explicitly
released to the parent for candidate staging. `dev` remains
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.

Runtime paths changed in this corrected pass (older planner/radar/signal runtime
changes remain mapped in their existing ledgers):

- `cluster/Cargo.toml`; `cluster/src/live/{params,state}.rs`;
  `cluster/src/models/cluster_ui_state/{mod,defaults}.rs`;
  `cluster/src/camera/input/mod.rs`;
  `cluster/src/renderer/{mod,background,tokens,signal_display}.rs` and
  `cluster/src/renderer/hud/status.rs`.
- `ui-application/src/{lib,signal_display}.rs` (this pass adds only the display
  module registration to shared lib.rs); `msgq/src/vision/retained.rs`.
- `cluster/src/autorun/{controller,entry,usb}.rs` and
  `cluster/src/usb/wifi.rs` for refreshed-source runtime behavior.
- `signal-observer/src/{lib,entry,neural_runtime}.rs` selects the already written
  `qcom.rs`/`qcom_provider.rs` runtime adapters. Their runtime prerequisites are
  the existing `neural-qcom` Cargo feature, `runtime-package` verifier,
  `runtime-launcher/src/{package,bootstrap}.rs` digest handoff, and additive
  `model-runtime/src/qcom/{bundle,model}.rs` immutable snapshot admission APIs.
- Exact external selected firmware files:
  `tools/esp32s3_display/firmware/{CMakeLists.txt,main/usb_bridge.c}`.

Focused test fixture: `cluster/tests/data/wifi_lifecycle.json`; inline tests live
in the runtime modules above. No development-tool/converter/provisioning/replay
caller was changed after the user's scope correction.

CI run38770312946 at486df478 follow-up: UI job116544085302 reported a single
nonminimal-bool Clippy error in the added display reader. A named finite geometry
predicate preserves its guard semantics. UI connectivity116544085353's render
failure was an unavailable exact source Git path, not a pixel mismatch; the parent
owns pinned-object fetching. Startup-services116544084791's Korean text pixel
mismatch used retained baseline graphics helpers; its source renderer now installs
the existing pinned final graphics loader before importing startup widgets. Pixel
checks are unchanged and their corrected CI execution remains pending.

Inherited navigation integration tests mixed the final upstream HUD renderer with
dev-base test setup. The Python reference's unknown-road `--` guard was restored
from dev-base to preserve the same existing Rust behavior. Animation/cache tests
now follow the pinned final sampling/reuse contract and initialize its actual clock
state. All36 targeted HUD/navigation tests pass. No Rust HUD behavior was reverted.

Startup render follow-up: local existing `debug/examples/startup_ui_render` was
built2026-10-15 21:48, before the font fix. Its actual font log requests38,851
Korean codepoints (38,788found); pinned Python source requests39,228 (39,158found).
Current Rust `renderer::fonts::font_points` and tracked `ui-font-final.json` both
specify the exact39,228 codepoints. Thus the local rerun used a demonstrably stale
font atlas. Its layout/scroll JSON already matches source; all2,706 remaining
pixel differences have maximum channel delta1. No tolerance was changed and no
runtime fix was added for this stale-binary result. The parent explicitly deferred
local rebuilding to avoid interrupting the87-binary AGNOS candidate build; fresh
CI will verify the unchanged exact-pixel gate with the corrected source binding.
