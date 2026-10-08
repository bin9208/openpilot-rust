# Native Panda runtime conversion (#175)

Issue [#175](https://github.com/bin9208/openpilot-rust/issues/175) is part of the
approved full-runtime conversion [#1](https://github.com/bin9208/openpilot-rust/issues/1).
The native core/supervisor component has reviewed host and ARM/QEMU evidence.
No physical Panda, USB/SPI device, C3X, vehicle CAN
or firmware-flashing operation has been used. Registered daemon selection remains
unchanged. Native core and supervisor checks are intermediate evidence;
the whole-runtime startup/log-upload gate remains incomplete. The catalog now
records both native executables as candidates, while preserving the disabled
standalone core descriptor and the always-enabled supervisor descriptor.

## CAN codec and SPI alert state

`rust/crates/pandad` contains a safe Rust CAN encoder/decoder and the source
`PandaSpiAlertTracker` state. The encoder retains the packed six-byte header,
all sixteen legal CAN/CAN-FD lengths, address/extended bit semantics, per-Panda
four-bus selection, XOR checksum and the 256-byte soft batching boundary. The
decoder retains partial input, rejected/returned bus offsets, already decoded
frames before a later checksum error, clearing the rest of a failed batch and
the caller's required communications-reset decision. Invalid outgoing lengths
produce a fatal typed error where the original asserts; they are not padded or
sent. Irrelevant-bus packets are skipped before payload validation, as in source.

The SPI alert tracker preserves five-second onroad arming, the ten-second
recovered-error window, three-event threshold, one-second confirmation and the
once-per-drive capture latch. Its unsigned time arithmetic matches the source,
including explicit wraparound cases. No scheduling, safety, retry or error
threshold is changed.

## Original-source comparison

`build_pandad_protocol_source.py` compiles the unchanged original `panda.cc` and
`spi_alert.h` with the full original generated cereal C++ schemas. The test-only
facade exposes protected/private state without rewriting the methods. It
substitutes only a transport reset recorder and logging sink; actual transport
semantics remain subsequent work. Source and generated-schema inputs are hashed.

`check_pandad_protocol.py` compares every byte of every output chunk, every
decoded frame, remaining partial bytes, reset/log effects and complete alert
state. Independent fixture checks require all 1,536 intended valid receive-flag
cases to decode one frame without checksum failure. The final corpus contains:

- 664 outgoing batch cases covering all legal lengths, all 256 source buses,
  address boundaries, large batches, multiple bus offsets and seeded input.
- 2,380 receive cases covering all bus/rejected/returned flags, fragmentation,
  all checksum-byte values, corrupt batches, subsequent recovery and seeded input.
- 130 alert sequences with 30,720 state transitions, threshold boundaries,
  repeated captures, ignition changes and unsigned time wraparound.
- Seventeen invalid outgoing lengths. Original processes exit with SIGABRT;
  Rust exits with a typed fatal error. Neither emits a result. The same lengths
  on an unrelated bus are ignored by both implementations.

All comparisons passed on x86_64 and with the aarch64 Rust executable under
QEMU. The original-source executable also passed the same comparison with
AddressSanitizer, UndefinedBehaviorSanitizer and leak detection enabled. Strict
all-target Clippy, Ruff, formatting and diff checks passed. This is host/emulated
evidence, not AGNOS, physical transport, drive or performance acceptance.

Local evidence base: `.analysis/scratch/2026-10-01-rust-pandad/`.

| Artifact | Result |
| --- | --- |
| `source-protocol-2/manifest.json` | Unchanged original C++ build and provenance |
| `protocol-flags-final/report.json` | Exact final host corpus |
| `source-protocol-asan/manifest.json`, `protocol-asan/report.json` | Sanitized original-source corpus |
| `protocol-arm/report.json`, `protocol-arm-build.log` | Exact emulated ARM64 corpus and build |
| `protocol-clippy.log` | Strict native package lint pass |

Host native example SHA256:
`ac28c03f739b79a016c568c09997b598d1bb151a013b97d951d31b1ead01c539`.
ARM64 example SHA256:
`5d6919c145307ea298285d3db72fa182f04115ed6bd78ae0c2de9848fbeb4aec`.
The checker takes `--source SOURCE_BINARY --binary RUST_BINARY --output FRESH_DIR`;
use `--qemu EXECUTABLE --sysroot DIRECTORY` for the emulated ARM64 lane. The
source builder takes existing Cap'n Proto and json11 prefixes; it installs no
dependencies. Invalid-input children disable core-file generation while retaining
their exit status and stderr.

## Safety configuration

The native `Safety` state machine now preserves ELM327 initialization, primary
versus secondary OBD multiplexing, firmware-query and ControlsReady gates,
onroad/offroad resets, per-Panda safety model/parameter/alternative-experience
commands, fallback SILENT for extra Pandas, and the source's log ordering.
Unknown safety-model ordinals and signed alternative-experience conversion retain
the original wire semantics. The fixture uses actual native Params files and
records transport commands; the C++ oracle compiles unchanged PandaSafety and
Params sources.

The host and emulated ARM64 comparisons each pass 897 scenarios / 4,499 steps:
889 have exact state, commands, logs, Params and parsing outcomes. Eight explicitly
separate malformed-input cases cover [#176](https://github.com/bin9208/openpilot-rust/issues/176),
an inherited `AlignedBuffer` defect: it exposes an extra partially or wholly
uninitialized word beyond the actual input. The native strict reader rejects the
truncated CarParams before configured safety commands and then exactly matches
the original's normal recovery on a complete message. These eight cases are not
reported as malformed-input parity.

A controlled original-source allocation experiment confirms the defect without
physical transport. A complete 312-byte configuration gives identical commands
under allocation fills 0x11 and 0x22. Removing its last eight bytes makes the
original choose second-Panda safety parameter 4369 or 8738 from the missing word;
Rust rejects both. Model 17 remains present in the input. The inherited C++
source is retained unchanged as an oracle. The diagnostic fixture alone controls
allocator contents, only for the bounded reproduction.

Evidence under the same local base:

- `source-safety-poison/manifest.json`: source, schema, fixture and binary hashes.
- `safety-host-final/report.json`, `safety-arm-final/report.json`: exact scenarios
  and the eight separately asserted guard/recovery cases; complete JSONL captures.
- `safety-padding-final/report.json`: both controlled allocation fills and normal
  input control; source and native commands retained.
- `safety-arm-build.log`, `safety-clippy.log`: bounded ARM build and strict
  all-target lint checks. Python Ruff and diff checks also passed.

## USB ownership and retry behavior

`openpilot-panda-usb` ports the original USB enumeration, serial selection,
configuration/claim, transfer retry and cleanup policy to Rust. The external
libusb-1.0 library remains a native dependency. Library, context, device-list and
handle lifetimes are owned; a per-handle mutex serializes synchronous transfers,
with source-compatible atomic connection/health flags. Buffer lengths are checked
before the C ABI and borrowed through each synchronous call. Runtime callers use
the fixed system soname; choosing an alternate library requires an explicit
unsafe ABI contract. Descriptor size/alignment/field offsets are compile-time
contracts. This follows libusb's documented [thread-safety and resource-release
requirements](https://libusb.sourceforge.io/api-1.0/libusb_caveats.html).

The source comparator compiles unchanged `panda_comms.cc` against the real pinned
libusb header, replacing libusb with a recorded ABI implementation. It covers
serial bytes including embedded nul, multiple and irrelevant devices, 105
connection failures, partial transfers, timeout/drop, overflow health latching,
disconnect behavior, retries and every cleanup path. Listing retains its static
context behavior, including no initialization retry after the first failure,
partial results before an error and later recovery. Diagnostic events preserve
their source messages; fixture time advances past the log-rate window. The
existing shared cloudlog rate limiter still belongs to final daemon composition.

Final host, ARM64/QEMU and combined Rust/C++ sanitizer runs each compare 948
scenarios / 3,886 scripted operations, plus separately asserted listing startup
failures. Real 2/4/8-thread callers yield maximum one active transfer for both
implementations. A fixture control which bypasses the application lock reaches
eight concurrent calls, confirming the backend does not conceal missing locks.
This tests owned ABI fixtures only, never physical USB or CAN.

The combined sanitizer lane instruments the Rust wrapper/example with nightly
AddressSanitizer and the original C++/ABI fixture with AddressSanitizer and
UndefinedBehaviorSanitizer; leak detection remains enabled. The prebuilt standard
library and external json11 internals are not fully instrumented. Miri was attempted
and stops at unsupported `dlopen`, not an observed UB report; native modules use
the explicit `native-skip-miri` feature. This is not a Miri pass.

Evidence under the local base:

- `usb-host-final/report.json`, `usb-arm-final/report.json`: exact full results,
  raw input/output and binary/library hashes.
- `source-usb-concurrency/manifest.json`, `source-usb-arm-2/manifest.json`: source
  and C ABI provenance. The ARM fixture uses the same pinned json11 version;
  `deps/json11-arm/manifest.json` records its public package URL and SHA256.
- `usb-combined-asan-final/report.json`: combined memory-instrumented comparison;
  `usb-rust-asan-owned-final-build.log` records the native build.
- `usb-miri.log`: unsupported foreign-call boundary. Earlier sanitizer-loader
  failures are retained; the final executable exports its ASan runtime and loads
  the matching standalone UBSan support for the C++ fixture.

## SPI and peripheral continuation (2026-10-02)

The interrupted SPI implementation remains in the same issue worktree. The
corrected-clock host and ARM/QEMU comparisons both pass 122 scenarios, 204
operations and 7,387 recorded calls. The safe SPI tests pass five cases and the
Linux boundary passes two owned-file tests. Earlier oversized source reads stay
separately tracked in #179; their undefined behavior is not a parity target.

The native peripheral controller now preserves the 20 Hz fan/IR policy,
100-update command refresh, camera frame-counter reset, conditional driver-view
Params read and strict one-second camera timeout. The C++ filter uses mixed
float/double evaluation; reusing the Python-style binary64 filter would change
the source arithmetic, so this controller retains the original rounding stages.
The unchanged source function, constants and FirstOrderFilter/map_val definitions
are compiled against recorded message/Params/hardware boundaries with ASan/UBSan.
All ordered commands and Params reads match across 38 scenarios / 30,240 steps.
Four focused behavior tests first failed for the absent module and now pass.

The native control-read adapter decodes the packed 58-byte health and 64-byte CAN
health structures without unaligned pointers. It preserves original zero-filled
short successful reads, missing results on negative reads, two-part firmware
signature checks, NUL-terminated identity reads and binary serial-log reads.
Against unchanged Panda methods and packed C++ structures, 1,746 scenarios /
2,490 operations match every field, exact interrupt-load float bits, command
order/parameters and returned bytes. Two packed-layout and four read-lifecycle
tests pass. The original comparator runs under ASan/UBSan. Strict package Clippy
passes; C++ language-server checks with the actual generated-schema/dependency
include paths also pass. Python type-server installation remains unavailable;
the existing Ruff/static and executable comparison paths are used.

New evidence: `.analysis/scratch/2026-10-02-port-resume/` contains
`peripheral-red.log`, `peripheral-source/{build,manifest}.json`,
`peripheral-differential/report.json`, `device-source/{commands,manifest}.json`
and `device-differential/report.json`, with raw inputs/results and binary hashes.
The peripheral and packed-device corpora also pass under ARM64/QEMU.

## Continuous native core (2026-10-02)

The native executable now owns the CAN send/receive, state, peripheral, serial
logging and SPI diagnostic workers, native Params/msgq/Cereal composition,
signal handling and connection lifecycle. Board scheduling retains main FIFO54
on core3, CAN-send FIFO55 and CAN-receive FIFO56; host fixtures do not exercise
privileged target scheduling. Rates remain 100 Hz for main/CAN receive, 20 Hz for
peripherals, 10 Hz for state and serial reads, and 2 Hz for peripheral publication.

The packed state publisher matches all serialized cereal bytes and ordered
effects in 16 scenarios / 4,632 steps. CAN I/O matches 2,167 scenarios / 3,857
operations, including fragmented packets, communications health, MAXOUT,
checksums, partial writes and bus selection. Both host and ARM64/QEMU pass.
Unknown wire enum ordinals and the original fault-list allocation semantics are
preserved. No physical transport is used by these comparisons.

`build_pandad_runtime_source.py` compiles the unchanged complete C++ daemon with
ASan/UBSan. An owned libusb ABI supplies synthetic devices and records all
control/bulk operations, without forwarding to system USB. The original msgq
and cereal Python peer drives the original C++ and native Rust processes; Python
is test infrastructure only. `/proc/PID/maps` verifies the selected ABI fixture
and absence of libpython in each tested daemon.

Nine continuous scenarios pass on original C++, x86_64 Rust and ARM64 Rust under
QEMU: offroad, onroad with trace timestamps, C3 DOS/red ordering, fake send,
disabled fan/SIGTERM, hotplug, disconnect, firmware mismatch and malformed
CarParams. The last case reproduced an extra Rust exit-relay command on a
parsing error; cleanup now follows the source's successful-loop exit path.
Malformed input fails with a typed native error instead of the source abort.

The saved host and emulated ARM captures also pass a separate semantic comparison: complete
publication transitions, log payload multiplicity, ordered changes per USB
control request, and every ordered CAN write. Run clocks, log process/callsite
metadata, repeated identical periodic samples and cross-thread log ordering are
explicit normalizations. Control reads are grouped by request/value/index;
every CAN-health read cycle must still contain buses 0, 1, 2 in order. A pipe
barrier in the owned USB fixture lets the peer synchronize after publisher
initialization and before the first state publication, avoiding a missed initial
sample in msgq. This does not claim deterministic scheduling or CPU
savings. Existing logging gains native virtual-filename and timestamp payload
APIs, with six real-ZMQ logger tests; serial messages retain `panda[index]`
metadata and unchanged text.

Evidence under `.analysis/scratch/2026-10-02-port-resume/`:

- `state-differential/report.json`, `state-arm/report.json`,
  `can-io-differential/report.json`, `can-io-arm/report.json`.
- `runtime-source-fixed/manifest.json`: unchanged source/build provenance.
- `runtime-source-gated/manifest.json`, `runtime-native-gated/manifest.json`,
  `runtime-arm-gated/manifest.json`: nine scenarios and complete captures.
- `runtime-gated-comparison.json`, `runtime-gated-comparison-arm.json`: captured
  semantic comparison and hashes.
- `runtime-proof/`: retained x86_64/ARM64 ELF files and native log collector.
- `pandad-combined-tests.log`, `pandad-runtime-clippy-fixed.log` and
  `panda-log-api-{red,green}.log`: focused tests and strict Rust lint evidence.

Retained x86_64 executable SHA256:
`fb7117bc0f9ecd335aa0a43abe474333cf75344248219822aa62bc7bd10c2c8b`.
Retained ARM64 executable SHA256:
`d422bb0875298ae5a2a80c63f11026d3bcb901ddaf6a2fafd128c3e2b7201f1b`.

Firmware/DFU supervision is implemented and undergoing composition checks. Firmware artifacts, libusb and
Linux driver interfaces remain explicit external dependencies. This component
has not been enabled in production startup or delivered for a vehicle test.

The first firmware library slice now implements MCU configuration/UID conversion,
application-sector bounds, 16-byte flashing, USB DFU status clearing, erase,
block padding, jump and bootstub recovery. Unchanged original Python function
bodies match 3,324 scenarios and 528,880 recorded commands on both x86_64 and
ARM64/QEMU, including transport faults and source-rejected inputs. It has not
been attached to physical USB/SPI.
The DFU direct libusb calls retain the binding's default zero timeout; normal
Panda calls retain 15 seconds, as confirmed in the
[python-libusb1 source](https://chromium.googlesource.com/external/github.com/vpelletier/python-libusb1/+/dab4906eac9ad61613e21b90ce7279204efa33ab/usb1/__init__.py).
Evidence: `firmware-differential-fixed/report.json`, `firmware-arm/report.json`,
full input/source/native JSONL captures, and retained
`runtime-proof/firmware-x86_64` / `runtime-proof/firmware-aarch64`.
The ARM64 firmware example SHA256 is
`15a8e2917861f58df316cab6f27779a7c9f6d7c27f3704875ee156056f86d1a0`.

The supervisor policy now ports the original wrapper's flash/recovery decisions,
stable Panda ordering, missing-internal retry escalation, startup health Params,
first-success resets, cleanup and child restart boundary. Unchanged `pandad.py`
and `panda_helpers.py` function bodies match 3,752 scenarios / 190,812 calls on
x86_64 and ARM64/QEMU. Every baseline call position receives four independently
injected error classes, including errors during cleanup, logging and child launch.
The comparisons include ordered logs and Params writes; traceback implementation
details are not compared. Native transport composition and real signal delivery
remain separate open work, so this is not a complete firmware daemon.
Evidence: `supervisor-{differential,arm}/report.json`, complete JSONL captures,
`supervisor-{red,green,clippy}.log`, and retained
`runtime-proof/supervisor-{x86_64,aarch64}`. The host executable SHA256 is
`01ccd36c4227df716aeb96d5d78f5ae36b8840134b5e8442e440d32cd2489cf1`;
the ARM64 SHA256 is
`2e9d33de0b1b83397a6073b2ca5ed98ae7c9601569a5a98f6a9561d06ae222fd`.

## Native firmware environment and supervisor continuation

The later nullable-serial corpus supersedes the initial supervisor and client
counts above: 3,999 supervisor cases / 195,459 calls and 2,474 client cases /
45,801 calls pass on host and ARM64/QEMU. The SPI firmware protocol comparison
passes 9,599 cases / 435,725 calls on both architectures. Original libusb binding
semantics are separately covered by 171 policy scenarios and 578 raw ABI cases.
USB first, SPI fallback, DFU discovery/recovery, signature-file tail reads,
Panda ordering, packet-version failures and transport cleanup remain explicit.

The native spidev adapter is compared with the actual preserved spidev 3.8 C
extension and unchanged `SpiDevice` and kernel-transfer Python bodies. Both host
and ARM64 native implementations pass 517 scenarios / 6,843 operational calls,
covering cached descriptors/speeds, configuration, flock, transfer limits,
short reads/writes and failures at each syscall boundary. The checker excludes
only trailing destructor close calls from the ordered comparison and retains
raw traces. The Python negative-descriptor `ValueError` and native `EBADF` are
classified as nonretryable I/O failures. The source's exceptional unlock can
leave its Python lock held; those fault cases end at the outer exception
boundary and do not establish subsequent recovery. Kernel ioctl `EINTR` remains
a failure, matching this original Python binding, while flock retries it.

`NativeEnvironment` composes the actual USB/SPI adapters with regular firmware
files and DFU helpers. `openpilot-pandad-supervisor` composes that environment,
Params, hardware control, structured native logs and the existing native process
launcher. The launcher closes inherited descriptors before executing the child.
SIGINT is recorded while setup or reset sleep finishes, forwarded to an active
child, and prevents the next outer restart. Setup-time SIGINT still permits the
current source-compatible child launch. Only the original wrapper's SIGINT policy
is ported; the core daemon has its own separately tested signal handling.

The unchanged Python wrapper/client bodies and actual native supervisor match
161 composed scenarios on host and ARM64/QEMU: real firmware and Params files,
owned libusb ABI calls, live structured-log IPC, actual exec, descriptor closure,
child exit 7 and missing-child failure. Three additional real-signal scenarios
match source log ordering and process termination: interrupt while waiting for a
child, interrupt during setup followed by child interrupt, and interrupt during
the three-second reset sleep. ARM runs use explicit QEMU launcher scripts around
the retained ARM process helper and fixture child, without installing binfmt.
They do not establish AGNOS scheduling or physical device behavior. The source
comparison scripts replace transport/discovery/Params boundaries explicitly;
they do not claim that Python source runs on ARM.

Evidence under `.analysis/scratch/2026-10-02-port-resume/`:

- `supervisor-nullable{,-arm}/report.json` and
  `firmware-client-nullable{,-arm}/report.json`.
- `firmware-spi-differential-fixed/report.json`, `firmware-spi-arm/report.json`.
- `firmware-usb-policy-{differential,arm}/report.json` and
  `firmware-usb-raw-{final,arm}/report.json`.
- `firmware-spidev-differential-final/report.json`,
  `firmware-spidev-arm/report.json`, `firmware-spidev-fixture-third/manifest.json`.
- `supervisor-native-release-check/report.json`, `supervisor-native-arm/report.json`.
- `supervisor-signals-release-check/report.json`, `supervisor-signals-arm/report.json`.
- `supervisor-arm-fixtures/{commands,manifest}.json`: native ABI and ARM child
  source/build hashes. `arm-process-launcher` and `arm-supervisor-child` name the
  exact retained helper/child artifacts used by the emulation checks.
- `pandad-supervisor-tests.log`, `pandad-supervisor-clippy-final.log`,
  `pandad-supervisor-final-host-build.log`, `pandad-supervisor-arm-build.log`.

Retained supervisor probe SHA256, host:
`1843fa022526a643ef1366a6bbd4ee9e5fa0d5f0fbd3a662b6ef1c13f0ba86b0`;
ARM64:
`673809436b7553cb86aab95ff819f839bcbe9b0e70003bfdfa5cc93c8d3bde7b`.
The actual supervisor CLI, native process helper and native core now pass three
connected scenarios on each architecture: offroad, onroad and two Pandas. Each
scenario starts the core, exchanges CAN/state/peripheral/log messages through
original Cereal/msgq, checks recorded signatures and configured safety commands,
interrupts the first core to cause a wrapper restart, then interrupts the wrapper
and verifies the second child is reaped. The native log collector remains on the
host in the ARM lane. Parent and child maps contain the owned USB fixture and no
libpython. Core signature comparison is explicitly bypassed for synthetic
firmware; its strict mismatch behavior has separate original-source coverage.

Composition also reproduced a CLI path defect: `--basedir` did not update the
default firmware directory. The default now follows the selected base unless
`--firmware` was supplied. The failure capture is
`runtime-composed-default-red/`; final verification covers explicit and default
firmware paths, all three scenarios and both architectures (12 executions).
`runtime-composed-final-{host,arm}-/{explicit,default}/report.json` contains the
captures and immediate artifact hashes; `runtime-composed-final-report.json`
additionally pins the ARM helper/core and QEMU binaries behind the explicit
launcher scripts. `runtime-composed-final-commands.json` records commands and
outputs. `supervisor-cli-default-clippy.log` records strict CLI linting.

Source discovery uses a Python set, whose enumeration varies with its hash seed.
Native discovery deduplicates the same device membership while preserving the
enumeration order; this is not a claim of cross-language hash-set ordering.
The supervisor's subsequent source-defined device sort and child serial order
are covered separately by the policy corpus.

The initial independent component review verified the recorded source, binary
and raw integration evidence, but found one missing observable effect: USB DFU
programming did not print the original per-block progress. The protocol oracle
had discarded source stdout and therefore could not detect that loss. The
review and reproduction remain in `.omo/evidence/pandad-component-review/`.

USB DFU programming now accepts a fallible progress callback, and the actual
`NativeEnvironment` recovery path writes each original message to stdout before
the corresponding block transfer. The extended unchanged-source oracle checks
the message and its position among transport calls, including stdout failures.
It passes 3,340 scenarios, 529,168 transfers and 112 progress attempts on both
host and ARM64/QEMU. Reports are `firmware-progress-final-{host,arm}/report.json`
under the evidence root above.

A separate executable now exercises the actual native USB DFU environment:
USB discovery and descriptor decoding, regular firmware-file reads, all-sector
erase, padded programming, reset, stdout and USB cleanup. Whole unchanged
`PandaDFU` and `STBootloaderUSBHandle` class bodies provide the reference, with
controlled USB boundaries. Both the host ASan fixture and ARM64/QEMU pass 118
scenarios, 3,010 transfers and 121 stdout lines, including empty/missing firmware,
both MCUs, nullable serial selection and failures throughout recovery. Reports
are `dfu-recovery-host-asan/report.json` and `dfu-recovery-arm/report.json`.
This closes the native USB recovery composition gap; native SPI DFU environment
recovery still relies on its separately tested protocol and ABI components.

The rebuilt supervisor CLI also passes the complete 12-execution
supervisor/core/IPC composition again with explicit and default firmware paths.
Its new captures are `runtime-composed-progress-{host,arm}/{explicit,default}/`,
with exact commands and underlying ARM artifacts in
`runtime-composed-progress-{commands,report}.json`. The initial receipt remains
preserved; `pandad-component-progress-receipt.json` records this correction.
The independent follow-up review marked the correction resolved after checking
the new hashes and raw captures (`.omo/evidence/pandad-component-review/followup/`).
`pandad-component-reviewed-receipt.json` links that review to the preserved
progress receipt; only this review-status documentation changed afterward.
Manager selection, complete normal startup, the existing upload path and user
device comparison remain full-runtime work. No CPU savings have been measured.

The later candidate-catalog probe passed both executable mappings and all eight
offroad/onroad, car/not-car predicate combinations. It also confirms that the
standalone `_pandad` source descriptor remains disabled while the `pandad`
supervisor remains enabled. Both retain the original Always predicate. Evidence
is `pandad-catalog/report.json` under the continuation directory; its native
probe executable and build/command records are preserved. The reviewed core and
supervisor production sources are unchanged; this registration does not select
either candidate in the production launcher.

## Fresh-runner CI integration (2026-10-03)

Merge `d416dabc` combines this Panda component with the reviewed Card,
Selfdrived and camera components. The required `panda-runtime` CI job rebuilds
original msgq and Panda references, downloads checksum-pinned json11 and
spidev source, and runs the existing full protocol, safety, device, state,
CAN, peripheral, firmware, USB/SPI and supervisor comparisons. The original
and Rust continuous captures must also pass the separate semantic comparator.
Both explicit and default firmware supervisor CLI compositions remain required.

`check_pandad_ci.py` records native/source identities and command outcomes.
It resolves the original runtime's static ZeroMQ dependency from the final
Cargo JSON build receipt, rejecting missing or ambiguous output instead of
selecting another build's cache directory. Disk guards precede each build and
comparison; failed artifacts are retained. The separate ARM job builds the
native core and supervisor without claiming target hardware execution.

Local integration checks passed 18 routing tests with 190 subtests, seven
Card/Panda tooling tests, 271 shell syntax checks and 30 CLI import/help checks.
The help checks use the existing pinned Card Python environment and retained
msgq binding; they are not fresh native execution. The independent static CI
review has no open findings. Exact-commit hosted execution is still required,
and the earlier component receipts remain the runtime evidence at this stage.

## Dev integration candidate (2026-10-07)

The issue-scoped candidate based on `ffc77c3f0be8a8dc14bea8039c8e228d7e78df2d`
reuses the completed Panda feature (`6fab8cf1`) from preserved integration
candidate `1dd955e1`. The four Panda crates, their original source-comparison
tools, required logging emitters and hardware float parser are transferred
without a new production algorithm. Workspace registration, candidate-catalog
mappings and the required Panda host/ARM CI blocks are the integration changes.
Existing dev/main push routing and all other runtime and native-memory gates
remain in place.

The component receipts above remain historical runtime evidence. This candidate
has only offline metadata, formatting, YAML/shell syntax and CI routing checks;
new native integration execution and exact-SHA Actions results are pending.
PR #222's native IPC consumer/observer compatibility fixes will be incorporated
through the parent's dev merge before publishing this candidate. Local Cargo
build/check/test/clippy and dependency installs were not run because the disk
guard has not reached its recovery floor. Complete manager startup, upload and
user device acceptance remain separate work; no production selection changed.

Docs-Not-Needed: isolated native runtime and host validation only; no user setting
or production process behavior change.

### Updated dev dependencies (2026-10-08)

The candidate now includes dev `2f5e5dd6956f0abc00f2d201e7b5452ada1563d5`,
after Carrot Navi PR #230 and Selfdrived PR #231 passed their required gates and
merged. The shared Selfdrived messaging API restoration is included. The four
Panda crates remain byte-identical to the prepared component at `87f9cea4c`.

The local integration checks passed 19 CI-routing tests, the focused Card/Panda
tooling checks, locked offline Cargo metadata and diff checks. The final dev
merge at `85c798959a42d6f0051360767133b22d814846b0` changes no tree content
from the already checked `466c4fc30`, so those checks are reused. No new local
Panda build or repeat of its complete source corpus was performed. The required
Panda host/source and aarch64 execution will run on this PR's exact head.
Normal manager startup/upload and device acceptance remain open.
