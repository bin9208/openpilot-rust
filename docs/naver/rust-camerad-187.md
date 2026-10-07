# Native camera runtime conversion (#187)

Issue [#187](https://github.com/bin9208/openpilot-rust/issues/187) continues the
approved complete runtime conversion in [#1](https://github.com/bin9208/openpilot-rust/issues/1).
Base: `87b71d714a1f63afa7ec5c2007a4d6503029e18c`. Sensor, exposure and packing
policies, request decisions and sensor/IFE/BPS packet builders now have Rust
implementations and host/emulated evidence. The native Spectra kernel adapter
and camera process now run with owned driver fixtures on host and ARM. Complete
manager startup/log-upload composition and the project-owned IPC boundary remain.
No physical camera, C3X or vehicle access is authorized.

Implementation order is sensor configuration/register policies and passive
timing, exposure control and command packing, Spectra resource/request/fence
lifecycle, then three-camera startup and actual native IPC composition. Original
C++ methods remain unchanged reference implementations. Generated constant data
must retain source hashes and compare against those source constructors.

Preserve source AR0231/OX03C10/OS04C10 behavior, including OS04C10 exposure/gain
grouping and delayed launch, original exposure bounds, core6 normal scheduling,
frame metadata, startup pairing and validity limits. Passive event timing must
remain diagnostic only. No source-defined frame rejection or error outcome may
be silently turned into a successful frame.

Kernel/AGNOS camera drivers, firmware and native transport dependencies remain
explicit boundaries. An FFI wrapper around existing project-owned camera code
does not complete this conversion. Sensor/memory/ABI host and emulated checks
are intermediate evidence; the complete normal-startup/log-upload candidate is
required before the user's first device comparison. No CPU savings are measured.

Disk checks precede every build/install/large copy. Reuse the coordinated target
cache, disable incremental builds and preserve required evidence and active
binaries. The initial checkout used approximately 342 MB and left 35.3 GiB free.

Docs-Not-Needed: isolated runtime port; no production process selection or user
setting change.

## Policy implementation and original-source comparison (2026-10-02)

`rust/crates/camerad` owns static sensor data, exposure register generation,
exposure scoring/control, image sampling, NV12 allocation layout, CDM command
packing and passive event timing. The package currently forbids unsafe Rust.
The static tables are regenerated from unchanged original sensor constructors,
with the source file hashes and MIT provenance embedded in the generated file.
There is no runtime call into the original camera implementation in this crate.

The comparison adapters compile the actual three sensor translation units,
CDM implementation and NV12/timing headers. Exposure and histogram adapters
insert verbatim original method bodies around controlled sensor, Params and
write boundaries. Their scope is policy behavior; they do not emulate physical
sensor operation. All images and input streams are synthetic.

| Evidence | Host | ARM emulation |
| --- | --- | --- |
| Sensor configuration/address/register/score comparisons | 535,020 pass | 535,020 pass |
| Continuous exposure scenarios / steps | 99 / 54,108 pass | 99 / 54,108 pass |
| Passive timing observations | 5,007 pass | 5,007 pass |
| NV12 layout cases | 1,644 pass | 1,644 pass |
| DMI / continuous / random command cases | 180 / 139 / 139 pass | 180 / 139 / 139 pass |
| Histogram image/region cases | 183 pass | 183 pass |
| Rust regression tests | 9 pass | 9 pass |

Sensor register coverage includes every valid exposure time, initialized gain
entry and conversion-gain choice, plus signed debug-time boundary cases. The
60,000 score cases compare binary32 bits. Exposure comparisons include complete
state, sampling rectangles and ordered register writes, dark/light transitions,
disabled frames, frame-number wrapping, manual string parsing and recovery from
parse errors. The histogram lane includes all 81 original tone patterns.

ARM initially exposed a real one-ULP port discrepancy. `SConstruct` selects
Clang, `-O2` and `-mcpu=cortex-a57` for larch64, which contracts multiplication
and addition in these expressions. Rust now explicitly preserves those fused
operations on aarch64 and the non-fused host behavior. A second regression
preserves which term of the three-frame EV sum is rounded before the fused
operation. Both failures remain recorded; acceptance tolerances were unchanged.
The ARM reference uses local Clang 18.1.3 with the repository's target options.
This is a pinned reference build, not confirmation of the installed AGNOS
compiler, ABI, camera driver or hardware.

Final retained policy reports are under the local evidence root
`.analysis/scratch/2026-10-02-port-resume/`:

- `camera-fma-x86_64-{sensor,misc}/report.json`
- `camera-ae-fma-order-host/report.json`
- `camera-fma-aarch64-{sensor,misc}/report.json`
- `camera-ae-fma-order-full/report.json`
- `camera-nine-{host,arm}-tests.log` and `camera-clippy-final.log`
- `camera-source-arm-clang-commands.json`, including exact compiler invocations
- `camera-proof/`, retaining the executed source and Rust binaries
- `camera-policy-receipt.json` and `camera-policy-source.tar.gz`, preserving the
  source snapshot, original-source hashes and verified report/binary references

The initial failed ARM GCC comparison and pre-fix Rust binaries are retained
separately. Earlier passing host reports are superseded by the retained reports
listed above. Generic aarch64 binaries are not device installation artifacts.

## Request and packet evidence (2026-10-02)

The request policy ports stale-event filtering, source startup synchronization,
request/frame gap recovery, IFE/BPS fence waits, requeue ordering and passive
timing. Verbatim original methods and the Rust policy match in 512 scenarios
with 72,493 steps on each architecture. Comparisons include complete policy
state, ordered I/O, fences and the shared startup synchronization map.
The policy review is recorded under the local
`.omo/evidence/camerad-policy-review/` directory.

Sensor probe/power, I2C, NOP and CSI-PHY builders are now Rust-owned. IFE and BPS
builders preserve command streams, lookup tables, packet offsets, patches,
plane metadata and acquisition resources. The complete final packet corpus
matches 3,281 cases and 5,869,496 bytes on host and ARM. Reports are
`camera-acquire-{host,arm}/report.json` under the evidence root above.
Original methods are extracted verbatim and compiled against original headers;
the controlled memory handles do not establish kernel or sensor behavior.

The independent ABI investigation compiled the actual original headers on host
and ARM: all 40 types, 194 fields and four ioctl numbers matched. The pure ioctl
envelope/result/retry policy independently matched 966 cases and 18,454 call
pairs on each architecture. Exact commands, source hashes and output are under
`.omo/evidence/camerad-kernel-contract-review/` and
`.omo/evidence/camerad-ioctl-review/`.

## Native adapter in progress

`rust/crates/camera-kernel` isolates Linux syscalls behind the explicit
`native-skip-miri` feature. The pure camera crate retains its unsafe-code ban.
The adapter currently includes device discovery, capability query, subscription,
session/device/link requests, fences, event decoding, image-handle imports,
memory allocation and exact-size FIFO packet reuse. Successful compilation and
the initial host/ARM tests are not source-equivalence approval for these new
modules. The master initialization lane passed an independent original
source/ABI fixture comparison: 138 paired comparisons across host and ARM.
The review is limited to device discovery, open/query/subscription and normal
FD teardown. Its report and pinned receipt are under local
`.omo/evidence/camerad-master-clone-fidelity.md` and
`.omo/evidence/camerad-master-review/receipt.json`.

The subsequent kernel-operation corpus passed 334 paired scenarios on each of
host and ARM: complete integer payloads, sensor/PHY acquire pointers, poll/event
data, interrupted-call boundaries, sync/BPS envelope differences, imported image
handles, allocation writes and exact-size FIFO reuse/zeroing. The host C++ source
and fixture use ASan/UBSan; the Rust comparison executable is not instrumented by
that preload. The ARM fixture runs under QEMU without sanitizers. Reports are
`camera-kernel-ops-host-final/report.json` and
`camera-kernel-ops-arm-0/report.json`. Fatal allocation cleanup and nested ISP/BPS
acquire-wrapper coverage subsequently passed independent review; this is not
approval of the complete camera lifecycle.

That comparison caught two real discovery regressions. The source sysfs-name
open does not retry EINTR, while device opens have bounded retries. The source
name reader retries interrupted reads indefinitely and uses a same-file
fallback after an initial non-EINTR read failure. The Rust discovery adapter
now preserves these separate policies. Rejected binaries and failures are
retained; the final reviewed revision is the discovery source snapshot and
binary pair pinned in `camera-kernel-discovery-record.json`.

The original source has ownership gaps on allocation and disabled-camera paths.
The Rust allocator owns exported FDs separately from kernel memory handles and
CPU mappings. Normal release unmaps and closes its exported FD before releasing
the kernel handle; imported image FDs stay borrowed. This deliberately avoids
the original persistent/temporary allocation FD omissions. Partial construction
uses RAII cleanup, whereas several original failures abort. These differences
must remain explicit in the later lifecycle review. The source's unchecked YUV
map result and reused output handle are preserved at the low-level boundary,
including a visible error result; they are not silently treated as a new valid
image.

Initial memory-boundary tests passed host AddressSanitizer. The OS syscall path
cannot run under Miri; the pure camera policies have passed strict provenance
and symbolic alignment checks. Neither check substitutes for driver-fixture
composition, real transport tests or eventual user device acceptance.

## Camera composition evidence

The Rust msgq API now includes an owned VisionIPC server, YUV image handles and
raw image allocation. It retains the existing native msgq transport through a
small CXX boundary. Buffer handles keep the native owner alive, range checks
precede copies, and safe borrowed descriptors cannot outlive their mapping
owner. Actual original-client reception, frame metadata/bytes, retained-buffer
lifetime after dropping the server handle, invalid bounds and final FD counts
passed on host and ARM. These runs use the host shared-memory backend; ION
driver behavior remains unverified. Logs are
`camera-kernel-vision-host-tests.log`, `camera-kernel-vision-clippy.log` and
`camera-kernel-vision-arm-tests.json` under the evidence root.

`rust/crates/camerad-runtime` now composes sensor probe order, initial I2C writes,
start writes, request pokes and sensor/session release using the verified kernel
and packet APIs. Its source/native corpus passed 62 scenarios on each of host
and ARM, with
5,882 calls and 1,119 complete packet/payload snapshots, including disabled ports,
all three detected sensors, failed probes, nonfatal configuration failures and
EINTR retries. The source oracle extracts the original sensor methods unchanged;
the test wrapper opens fixture devices and explicitly releases sensor/session
resources. Thus it tests the sensor operations, not full `SpectraCamera`
destruction or production device discovery. Its report is
`camera-sensor-lifecycle-{host,arm}-0/report.json`. Full camera composition
remains pending.

ISP/BPS composition now passes 30 paired cases on each of host and ARM/QEMU:
all three sensors, raw/IFE/BPS paths, one and eighteen buffers, signed request
boundaries, and interrupted acquire/configure/allocation calls. The oracle
extracts original allocation and configuration methods unchanged and captures
live nested resources, submitted packet/command bytes, and every mapped byte
at release. Native cleanup additionally closes the original persistent exported
FD leaks and unmaps the temporary BPS config; the checker verifies each extra
cleanup and the unchanged config bytes explicitly. Host source and fixture use
ASan/UBSan; Rust and ARM execution are not sanitizer-instrumented. Reports are
`camera-isp-lifecycle-host-0/report.json` and
`camera-isp-lifecycle-arm-1/report.json`, pinned with source/binary hashes in
`camera-isp-record-v1.json`. The following composition now joins these resources
to PHY, links and frame images; complete daemon startup remains separate.

The independent kernel/Vision review is recorded in local
`.omo/evidence/camera-kernel-vision-review/REVIEW.md`. It reproduced and closed
an exported-FD0 rejection leak, checked four allocation cleanup paths, ran eight
Vision restart/reconnect cycles with stable FD counts, compared twelve actual
nested ISP/BPS pointer calls with the frozen original oracle, and audited all
668 frozen kernel receipts. New cleanup/pointer probes ran on host only. A
receive timeout retaining `connected=true` is inherited native behavior;
explicit reconnect was verified. The caller still owns reconnect policy.

`CameraPort` now joins sensor/ISP/PHY start, link activation, actual VisionIPC
image allocation/import, request/fence operations and shutdown. Its frozen
source/native gate passed 32 lifecycle/error scenarios and eight ownership
scenarios on each of host and ARM/QEMU. The 80 cases include disabled and absent
sensors, failed sensor writes, partial ISP/PHY/raw-map initialization, and every
allocated image even when import fails. Native cleanup of original disabled-path
leaks is individually checked against acquired resources and exact payloads.
Failed cleanup syscalls count as attempted cleanup, not proof of hardware
reclamation. The local receipt is
`.omo/evidence/camera-port-lifecycle/receipt-v4.json`, with the independent review
and frozen pre-SystemClock source beside it. The probe's outer-owner destruction
order is explicit and differs from the actual daemon declaration order.

`FrameState` composes the existing exposure policy with the three camera-state
Cereal messages. The unchanged original `sendState` reference passed 1,080
synthetic frames on each of host and ARM, including pre-AE metadata, raw-image
decimation, manual exposure parsing failures, wraparound IDs, post-AE state and
register writes. Reports are `camera-state-{host,arm}-full/report.json`; source
and binary identities are in `camera-state-record-v1.json`. This frozen adapter
gate uses controlled image/transport boundaries. The subsequent runtime keeps
manual Params reads lazy at their original partial-update phase.

The native process now opens all three cameras in source order, starts the
VisionIPC listener before the sensors, handles source event/requeue/synchronization
policy, publishes Vision frames before old-exposure camera state, updates exposure
and sensor registers, then sends camera-state IPC. It retains source affinity,
environment-presence switches, SIGINT/SIGTERM/SIGPWR and normal poll-error exit.
Diagnostics use the existing native logmessage transport and actual Rust sites.
The previously omitted unconditional fence-wait clock reads are restored around
stress evaluation and ioctl; seven focused request/ordering tests pass. Three
frame-state tests include lazy Params and the source partial update on failure.

The frozen host SystemClock gate passed 54 cases/151 ordered observations of the
original stress function, libc random sequence, numeric-prefix parsing, errno
preservation and real BOOTTIME bounds. Its independently hash-verified receipt is
`.omo/evidence/camerad-stress/receipt-host-v1.json` (846 files). Actual-main host
checks subsequently passed eight cases on each of host and ARM with 212 paired
camera-state/VisionIPC publications (105 host, 107 ARM): all three normal shutdown
signals, disabled/missing road camera, wait failure/requeue, terminal poll error
and a publication-order barrier. Captured fields and sensor writes
match the original C++ frame-state/exposure oracle exactly using the actual
frame metadata. The real clock is not replaced or numerically tolerated.
Lifecycle resource auditing passed in all sixteen cases. The barrier blocks the
first exposure ioctl and observes one Vision frame with no camera-state messages
before release, followed by the matching state message. Host binary v2 is
`3fd20737e68f19e61bf29a13784708f4e0cdca5c0111284924d489fd812a22d4`;
the independently verified 831-file integration seal is
`.omo/evidence/camerad-runtime/receipt-v1.json`. ARM stress also passed 54 cases
and 151 observations. Source main was reviewed; actual-main execution used
owned driver fixtures and the retained original frame-state/exposure oracle.

The separate ION ARM binary is
`921786dd2b7ef5b281045239217b90e70ef5edf8264b7b929aca64e1ab48d47f`.
Its offline ABI check extracted the actual libraries from the manifest-pinned
AGNOS 19.8-carrot-bt1 system image after all three image hashes matched. Every
required symbol version and all 540 strong symbol references across the binary
and its dependency closure resolved. The original image loader successfully
listed dependencies under QEMU. This establishes dynamic linkage against that
image, without claiming ION or camera hardware execution. The local record is
`agnos-19.8-abi/abi-report.json` in the parent continuation evidence directory.

The native runtime currently copies full NV12 buffers for exposure sampling.
That added per-frame cost has not been measured on a device. Original
LOG_RAW_FRAMES with road IFE has no raw buffer; Rust reports a typed failure
where the original dereferences the absent buffer. This is not supported raw
logging and is not silently accepted.

## Fresh-runner validation integration (2026-10-03)

The camera branch now includes the Card and Selfdrived integration through
merge `2df46043`. The required `camera-runtime` job builds the original Python
VisionIPC peers, generated schemas, sensor/exposure/packet references and
driver fixtures directly from this checkout. `check_camerad_ci.py` records the
source and executed-binary hashes, commands, output and failures. It compares
generated sensor/BPS constants, all existing host policy/lifecycle/state/stress
lanes, and the eight continuous runtime scenarios including publication order.
The separate ARM build retains the generic ELF before rebuilding with ION.

The initial local CI integration checks passed 17 routing tests with 179
subtests, five Card fixture/bootstrap tests, all 263 workflow shell blocks, and
14 original-source generator invocations. The corrected IPC/schema import
preflight passed using the pinned Card Python environment and a retained
original VisionIPC binding; this is an import check, not a fresh binding build
or a new runtime comparison. The CI runner has not yet completed on a fresh
hosted runner at this stage. Existing component receipts above remain the
runtime evidence until exact-commit hosted results are recorded.

## Remaining camera work

Complete full manager startup/log-upload integration and the remaining
project-owned IPC implementation boundary. Generic host/ARM simulations and
offline ION linkage do not establish target ION or camera-driver behavior.
Preserve source reset, failure and startup synchronization
behavior, including the strict timing bounds. Camera issue #187
and overall runtime issue #1 remain open; none of these component results
establishes device acceptance or CPU/thermal savings.


## Dev integration candidate (2026-10-07)

The candidate based on `87f9cea4` reuses the completed camera feature
`ae604544` and CI recipe `4d964458` from preserved integration `1dd955e1`.
The camera-kernel, camerad and camerad-runtime crates and their original helper
closure are transferred without new camera policies or options. Current native
VisionIPC APIs and binding preparation tools are reused. Workspace, candidate
catalog and inventory registration and the original camera host/generic+ION ARM
CI blocks are added while preserving every existing runtime and memory gate.

The original host/ARM/kernel/ISP/signal receipts above remain historical runtime
evidence. Only locked offline metadata, formatting and short YAML/shell/CI
routing checks are performed for this candidate; new builds, camera execution
and exact-SHA Actions results are pending. No local build, install or device
access occurs while disk recovery is pending. Full startup/upload integration,
physical camera/ION operation and user device acceptance remain separate.
Production process selection is unchanged.
