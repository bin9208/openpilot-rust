# First user drive and runtime optimization audit (#293)

Issue: [#293](https://github.com/bin9208/openpilot-rust/issues/293).
Recorded 2026-10-14 KST against local source
`f3446f65e1cbd894aba1aac7e724c12e6d89ff61` on
`codex/fix-294-encoder-rotation` (initial inspection branch: `codex/feat-293-runtime-drive-audit`).

The user reports normal driving and requests inspection of the connected
device's internal logs, further Rust optimization and removal of remaining
runtime Python. This authorizes reading the connected device for this task,
superseding the earlier pre-handoff no-connect instruction for that inspection.
The reported drive was subsequently inspected using the full cereal schema; results follow below.

## Source inspection

| Path | Observed behavior | Interpretation |
| --- | --- | --- |
| `launch_openpilot.sh` | On AArch64, runs `rust/tools/ensure_runtime.sh`, then execs `rust/bin/openpilot-launch` | Normal checked-in entry selects the native launcher |
| `rust/crates/runtime-launcher/src/bootstrap.rs` | Verifies the package, starts native startup/model services and `openpilot-manager` | No project Python interpreter invocation in this inspected startup path |
| `rust/crates/manager-catalog/src/binding.rs` | Resolves candidate binaries and requires executable ELF headers; `NotPorted` returns no binding | The `SourceProcess::Python` provenance label does not dispatch Python |
| `rust/crates/manager/src/installed.rs` | Starts resolved native or persistent commands using `openpilot-process-child` | Actual device process mappings still need inspection |
| `launch_chffrplus.sh` | Retains the original Python bootstrap and manager path | Not called by the checked-in normal entry; device entrypoint and external callers remain to be checked |
| `rust/tools/seal_runtime_package.py` and `package_runtime_release.py` | Build-time sealing and release construction | Python tooling is not evidence of target runtime Python execution |
| Optional `webjoystick` | Source module absent; sealing permits only the absent, disabled entry | Explicit unavailable-source boundary, not a Python fallback |

This is a targeted source trace, not an exhaustive runtime dependency proof.
Arbitrary support-terminal commands, optional helpers and OS-provided programs
need separate classification. Preserve source oracles, build tools, licensing
and tests while identifying runtime removal candidates. Do not delete Python
files merely because they exist in the checkout.

Historical component inventory language still includes pre-integration limits.
Do not infer the installed version, active features or device coverage from
those status labels. The private earlier boot-recovery record identifies
`a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31` as its recovered source/package,
but that does not establish the version of this drive. Release availability
[#290](https://github.com/bin9208/openpilot-rust/issues/290) and persistent
service replacement [#287](https://github.com/bin9208/openpilot-rust/issues/287)
remain relevant to subsequent candidate installation.

## Inspected drive

The user granted the host USB ACL. Restarting the host ADB server made it
usable. Read-only inspection confirms source, installed manifest and drive
`initData` all identify `a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`.
Current normal startup uses `launch_openpilot.sh`. All 22 currently running
project executable mappings use the matching native generation, with no deleted
executable mappings. No installed device code, setting, service or runtime was changed.

Thirty full rlogs (454,531,890 compressed bytes) cover 1,807.751 seconds relative
to the first CAN message. The vehicle is an Ioniq 5 PE on a tizi device. Raw logs,
identifiers, hashes and analysis scripts remain in ignored local scratch; none
are included in Git. The final segment's hash matches its device source, but its
Zstandard stream is unfinished and its final cereal record contains 528 of 904
expected bytes. Complete preceding messages were parsed; the incomplete tail is
excluded. This establishes an incomplete recorded tail, not its power/stop cause.

The performance window uses complete segments 1–28, excluding initial startup
and the truncated final segment. Whole-route active control is true in 76.864%
of selfdriveState samples; this percentage is sample-based, not integrated time.

| Observable | Result and scope |
| --- | --- |
| CAN | 180,522 carState messages; zero canValid failures or canTimeout flags |
| Control publications | Window mean 9.994 ms, p99 14.206 ms, max 15.981 ms; all window carControl/controlsState messages valid |
| Camera SOF | All three streams have consecutive window frame IDs; road/wide/driver maxima 51.119/51.168/51.166 ms, no interval above 75 ms |
| Driving model | 34,026 window messages, all valid; execution mean 28.441 ms, p99 30.786 ms, max 31.926 ms |
| Model cadence | 114 window publication gaps above 75 ms, max 251.411 ms; 100 nonconsecutive model frame-ID transitions despite continuous camera publication |
| Pose | Window inputsOK and message validity remain true; sensorsOK is false on 8 messages. Whole-route 18 invalid/inputsOK=false messages occur during the first ~4 seconds |
| Thermal | All 3,062 window deviceState samples are green; hottest CPU sensor mean 71.736 C, max 78.5 C; hottest GPU sensor max 75.2 C |
| Memory | deviceState usage mean 50.033%, max 52% |
| Driver monitoring | Neither driverStateV2 nor driverMonitoringState appears in the full-schema count. Driver camera recording does not establish active DM inference |

Two controlsMismatch events occur during startup at about 4.66/4.87 seconds.
A single FCW event occurs later; this inspection does not judge its traffic
correctness. Model gaps concentrate in the initial minutes but also occur later;
encoder restart causality has not been established. No matched original-runtime
baseline exists, so these figures are not measured Rust savings.

## CPU priorities and Python inventory

CPU accounting differences use (PID, startTime), discard negative/reset counters,
and aggregate cpuUser+cpuSystem over the common procLog window. One logical core
is 100%; these numbers are not percentages of all eight cores. Native encoder
instances are aggregated by executable; its main and optional modes share that
name.

| Process | Mean core-equivalent CPU |
| --- | ---: |
| pandad | 45.86% |
| loggerd | 27.81% |
| UI | 19.16% |
| driving-modeld | 17.37% |
| camerad | 15.32% |
| locationd | 14.66% |
| encoderd instances combined | 14.48% |
| card | 14.15% |
| htop (additional diagnostic workload) | 11.73% |
| CarrotMan | 7.90% |

Whole-core deviceState means for cores 0–7 are
43.83/44.18/39.88/74.03/15.20/18.01/46.98/16.82%. CPU 3 is the busiest
average core in this drive. Preserve the current priorities and placements while
localizing cost; do not infer free scheduling headroom from selected-process
sums alone.

Separating the same procLog deltas gives pandad user/system costs of
4.94/40.92% and loggerd 19.53/8.28% of one core. Thus most observed pandad
cost is kernel CPU time; this does not identify which syscall consumes it.
The inspected SPI protocol retains the source's busy turnaround waits and
ACK transactions. No SPI phase/slow/checksum/timeout diagnostics matched the
inspected logMessage prefixes in the performance window. That absence is not
a syscall profile. Loggerd retains source compression level 10, and its zstd
provider already buffers input internally. Neither observation justifies
changing transport cadence, safety timing or compression quality. Further
optimization needs phase/syscall profiling or a representative benchmark;
no speculative performance change was made to these two processes.

All Python executable records in the drive and current process inspection belong
to five AGNOS services: `/usr/comma/brightnessd.py`, `magic.py`, `power_monitor.py`,
`varwatch.py` and `power_drop_monitor.py`. Their combined observed cost is about
0.20% of one core. No project Python process is observed. Two-second procLog
sampling cannot exclude very short-lived helpers, and unexercised options still
need source/target coverage. The observed OS services are outside the approved
project-owned runtime conversion; they were not stopped or removed. The old
Python source/build/oracle tree is not evidence of active runtime cost.

## First actionable defect and next gates

Manager logs and managerState identify 29 main-encoder SIGABRT exits and restarts,
one at each approximately minute-long rotation. Loggerd times out waiting for
camera packets, rotates, and receives restarted segment-zero packets. Each
following main video stream drops 29 non-keyframes before recording resumes;
qRoad drops 14. This recording defect is tracked separately in
[#294](https://github.com/bin9208/openpilot-rust/issues/294).

A focused original/native driver-boundary fixture exposed a source-parity bug:
Rust rejects signed timestamps on buffers whose timestamps the source does not
use. The correction and tests are recorded in
[encoder rotation investigation](rust-encoder-rotation-294.md). An explicitly
approved offroad source-oracle probe then observed EOS timeval `{-1, 999999}`
on both rotations and completed 16 synthetic frames normally. Replaying that
exact timestamp reproduces SIGABRT in old Rust and passes in corrected Rust.
This establishes the physical driver's triggering value and the faulty conversion
path, but corrected-runtime driving acceptance remains outstanding.

The isolated probe was bounded to 20 seconds, used its own ION buffers and no
camera/control/IPC interfaces, and was removed after its outputs were retrieved.
Offroad state and absence of the temporary process were checked afterwards.
Required exact-SHA CI, a matching release and corrected-runtime target rotation
validation remain separate gates. The installed runtime continues to be the
inspected original candidate.

Available host space remained above 394 GiB; each build checked reserve and
reused the coordinated target cache with incremental compilation disabled.
GitHub tracking uses the connected app because the local CLI is unauthenticated.

Docs-Not-Needed: engineering investigation and internal encoder parity fix;
no user setting or intended user-visible behavior changed.

## Model publication follow-up: redundant durable USB status writes (#310)

Read-only follow-up on 2026-10-14 used the same private captured full-schema
rlogs and exact drive source `a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`.
No device was contacted. There is no standalone native rlog inspection executable
in the current tools; the existing private full-cereal-schema analysis was extended
to retain model/camera timestamps and INFO runtimeTiming records. Raw captures,
per-frame records, scripts and correlation details remain in the existing ignored
`2026-10-14-drive-audit` scratch directory.

All 100 nonconsecutive model publication transitions have intervening
`camera dropped ...; advancing model history` warnings. All 34,026 steady model
messages pair equal main/extra frame IDs; the maximum matched road/wide SOF skew
is 1.519 ms, comfortably inside the unchanged 20 ms pairing limit. The gaps split
into 80 jumps of three frame IDs, 13 jumps of four, seven jumps of five, and
14 gaps with consecutive IDs. Inference time on the 114 gap-ending publications
has median 29.106 ms and maximum 30.912 ms. Continuous camera publication does
not establish that model receive/copy/scheduling was timely.

Counting publications plus deduplicated prepare-only warnings reconstructs model
iterations. Cumulative counts agree with 1,758 of 1,759 diagnostic windows and
with the final cumulative count. One warning's log-service timestamp trails its
producer timing boundary by about 6.1 ms; retain this timestamp uncertainty.
Seventy-nine of the 114 gaps immediately follow a twentieth-iteration boundary:
45 of the jumps of three, all 13 jumps of four, all seven jumps of five, and all
14 consecutive-ID gaps. There are 1,707 eligible publication transitions after
such boundaries and 32,318 others. The remaining 35 gaps are jumps of three
between route-relative 69.015 and 139.584 seconds (segments 1–2).

At that exact boundary, `drivingd` calls `usb_selection::refresh` after recording
`FrameTiming`. Even with USB absent, refresh unconditionally writes UsbGpuPresent
and UsbGpuCompiled. `Params::put` writes a temporary file, syncs it, renames under
the Params lock and syncs the directory. Consequently that recurring durable work
is outside the recorded loop metric. This is an actionable source hypothesis for
[#310](https://github.com/bin9208/openpilot-rust/issues/310), not proof that those
writes caused every correlated gap or explain the other 35.

The early dropped-frame windows also have median maximum thread CPU 31.887 ms,
versus 8.564 ms in windows without drops. The installed source allocates and copies
both 4,804,608-byte camera buffers before pairing, including candidates later
discarded while catching up the nonconflated extra stream. This identifies another
measurement/test target, not a proven cause. Existing candidate buffer reuse has
not been measured on this drive. Preserve all exposure, pairing, drop/preparation,
recurrent-state and validity behavior while testing either hypothesis.
