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
executable mappings. No device code, setting, service or runtime was changed.

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
[encoder rotation investigation](rust-encoder-rotation-294.md). It is a candidate
for the observed SIGABRT, not yet the demonstrated physical-device cause.

An isolated synthetic-frame hardware probe has been built and its dependencies
checked with the exact AGNOS loader. User approval is pending because the older
AGENTS device-test prohibition goes beyond the newly authorized log inspection.
No probe has run on the device. Required exact-SHA CI, a matching release and
actual target rotation validation remain separate gates. The installed runtime
continues to be the inspected original candidate.

Available host space remained above 394 GiB; each build checked reserve and
reused the coordinated target cache with incremental compilation disabled.
GitHub tracking uses the connected app because the local CLI is unauthenticated.

Docs-Not-Needed: engineering investigation and internal encoder parity fix;
no user setting or intended user-visible behavior changed.
