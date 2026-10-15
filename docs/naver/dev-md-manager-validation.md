# dev-md manager, Params and Bluetooth integration

Source: `ajouatom/openpilot:carrot-wip` at
`e8e152dbcfd9574a1791ed12026d1880e87689b3`; native branch `dev-md` only.
This is intermediate engineering evidence, not a complete runtime or device
acceptance record. Stable dev stays at `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.

## Implemented contracts

- Add 21 source Params keys without silently applying unrelated defaults or
  removing settings before their consumers are ported. DM session-disable clears
  at ignition/manager startup, not ordinary onroad transitions.
- One-time DM migration records mode 0 and independently preserves legacy
  `DisableDM == 2` streaming intent only when the new streaming key is absent.
  Existing new values win. Invalid legacy integer conversion cannot mark the
  migration finished. Old DisableDM does not gate native DM processes.
- Keep the DM dispatcher alive onroad and for driver preview. Run its camera
  model only when persistent DM is enabled and session-disable is false, or
  driver preview explicitly requests it. WebRTC uses CarrotVisionEnabled without
  suppressing it for cluster HUD. Xiaoge requires onroad ShareData plus a
  session readiness latch.
- Xiaoge's latch compares each publisher's timestamp with its own onroad
  baseline; valid/current messages, configured Panda safety, engageability,
  valid CAN and ControlsReady must persist for 0.5 seconds. Observation gaps
  over one second reset settling. Offroad resets the latch.
- At ignition, block until the old dmonitoringd exits before clearing transient
  state; an old asynchronous session-disable write cannot race the reset.
- Bluetooth attention is a third independent journal. Eligible non-repeated
  physical tokens generate attention even when no cruise/lane action is mapped.
  Command throttling, holds and repeats retain their original semantics.
- Require both new Tesla wake Panda images in the native installed binding.

## Validation (2026-10-15)

44 coordinated host Cargo tests for manager, manager-catalog, Bluetooth and Params
pass, including their examples; all-target clippy with warnings denied passes.
Source/native comparisons pass:

- Manager: seven scenarios, exact lifecycle trace, Params bytes and environment.
  The fake old worker flushes a queued session-disable at stop; ensure-running
  asserts it is cleared before the replacement starts. Full source transition
  ordering is retained.
- Manager catalog: 63 ordered descriptors, 64 configurations, 25 environment
  snapshots, 11,202 predicate cases and 18 fatal-integer boundary cases.
- Bluetooth engine: 26 scenarios / 337 steps, exact command and attention
  journals, status history and connection decisions (UUID normalization only).
- Bluetooth journal: 801 operations, three independent channels, exact values
  and atomic rewrite decisions, mode 0600 and bounded 64-event retention.

The original Params Cython binding was rebuilt against the changed header and
its recorded source hashes were checked by the catalog comparator. Host tests
use isolated filesystem/IPC state. No device access or physical actuation.

Retained process_config.py/manager.py are scoped differential references: DM,
Xiaoge and stop-before-clear changes are from the pin; remaining startup/crash,
new-daemon and obsolete-YouTube changes are still pending. Bluetooth model.py,
daemon.py and monitoring/config.py match the pinned source exactly. Do not label
these scoped manager references a completed final upstream runtime.

Local logs are under `.analysis/scratch/2026-10-15-dev-md-resume/`; useful concise
results are retained here so reproducible binary caches can be removed.

The manager now uses the shared source lower-bound helper before starting
runtime consumers: invalid/nonfinite VEgoStopping repairs to 50, finite values
below 10 repair to 10 (stored hundredths of m/s). Its default_edges fixture
seeds 5 and observes 10 in both actual source and native initialization. All
seven manager trace comparisons pass again after this integration.

## 2026-10-15 resumed retirement validation

Removed final-source retired YouTube four process predicates/descriptors and
three canonical Params keys (CarrotYouTubeLive/Quality/Timestamp). Invalid integer
manager default fixtures now use existing ClusterHud/ShowDebugUI keys. A separate
source Params binding was rebuilt successfully at dev-md-resume/params-retirement;
the earlier binding was left intact for running source comparisons.

Root host test batch:72 tests passed across manager, manager-catalog,
carrot-server and native encoderd (root-host-tests-v2.log). This checks the removed
server/encoder/manager paths compile together; source catalog/HTTP/codec oracle
reruns and revised ARM binaries remain pending. Shared-library runtime paths and
an obsolete libjpeg CMake source cache were environment issues (explicit native
SDK LD_LIBRARY_PATH/libyuv path and cmake --fresh resolved them).

Fresh source catalog retirement comparison PASS:59descriptors,64configurations,
25environment snapshots,4,372predicate cases and6fatal cases. Seven wholemanager
scenarios pass exact traces/Params/environment on host and freshly linked
ARM/QEMU manager_trace. ARM manager binary also linked successfully. Cross-link
requires FFMPEG_DIR plus targetrpath-link for existing FFmpeg dependencies;
ffmpeg-sys cached host-search output did not invalidate on FFMPEG_DIR change,
so its ARM-only build cache was cleaned before retry. No source ABI relaxation.

Native application cleanup failure injection passes: failed support owner still
allows terminal/live/nav shutdown and propagates original error. Native V4L
source differential passes22cases after retired modes are removed. Software codec
comparison is running separately. These checks do not close full runtime gate.
