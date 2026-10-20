# Runtime module audit coverage (#296)

This records source areas actually inspected during the 2026-10-14 follow-up.
A focused regression or passing package test does not clear all internals of
that module. [The implementation ledger](rust-runtime-retirement-296.md) owns
integration/device evidence and the remaining Python-tool inventory.

| Area | Inspected scope and evidence | Remaining scope |
| --- | --- | --- |
| runtime-launcher | #298 watchdog ownership; #287 exact native service identity; #290/#297 atomic source/package startup handoff and full host launcher tests | Other startup internals; AGNOS/device restart and permission boundaries remain unvalidated |
| deleter | Mutable bookmark cache; #298 late/removed bookmark regressions | Other retention paths and target recording interaction |
| msgq | Zero-timeout notification wait; 52 package tests and Clippy | Other transport internals beyond the recorded focused checks |
| drivingd / model input | VisionIPC retained-frame ownership and allocation reuse; 14 integration tests | Remaining inference, recurrent state, target GPU and camera recovery paths |
| carrot-server, carrot-recovery, dashcam-upload, qcomgpsd | Python-source extraction in build scripts; source-free builds and output comparisons; native server/recovery maintenance transaction #290/#297 below | Remaining service internals and actual target activation/restart behavior |
| micd / PortAudio boundary | Callback abort, inactive stream and query-error owner handling; #300 RED/GREEN four native lifecycle tests, Clippy and seven-case original daemon gate passed | Target microphone behavior and device restart acceptance |
| carrot-navi | Completed connection task retention in shared JoinSet; #301 regression, native package 19 tests and Clippy passed | Protocol/service internals outside that focused defect; target behavior |
| panda-spi / pandad | SPI recovery audit assigned to another worker | Refer to its evidence before marking any path verified |
| manager / persistent generations | #287 native executable/inode/generation ownership; bounded pidfd retirement, live-helper retention, 44 original/native supervision scenarios and focused native tests | Other manager internals; Linux 4.9 needs normal reboot for old live owners; unreadable process identity conservatively retains generations |
| UI | startup-ui, ui-framework, ui-application, cluster, usbgpu, webcam, sdl2 | Internals not cleared by this audit |
| Connectivity | bluetooth/BlueZ, wifi, modem, lpa, registration, athena, jetlink, navd, webrtc, bridge, cweb-push, http-transport | Internals not cleared by this audit |
| Audio except micd | soundd, beepd, amplifier | Internals not cleared by this audit |
| Telemetry/storage/upload | statsd, proclogd, logging, logmessaged, journald, bootlog, tombstoned, crash-reporting, feedbackd, uploader, web-upload, loggerd, encoderd | Some prior focused evidence exists; broader internals are not cleared by this audit |
| Vehicle/control/planning | card, radarcan, radard, controlsd, selfdrived, plannerd, carrot-man, carrot-state, control-policy, control-tools, desire, can | Internals not cleared by this audit; radar changes require separate NAS verification |
| Sensors/model/device | camerad, camerad-runtime, camera-kernel, sensord, locationd, calibrationd, paramsd, torqued, modeld, dmonitoringd, dmonitoringmodeld, model-runtime, hardwared, hardware-control, hardware-info, panda-usb, panda-spi-linux, ublox | Internals not cleared by this audit |
| Runtime/support/tooling | runtime-core, runtime-installer, runtime-version, process-supervision, managed-entry, manager-catalog, agnos, updated, timed, lagd, monitoring, checkout-status, params, params-typed, cereal, messaging, log-format, jpeg, opencv-runtime, pocketfft, xiaoge, dev-tools | Native docs gate is verified separately; remaining internals and developer-tool conversion remain open |

### Locationd sensor admission (#373, 2026-10-19)

The wire decoder previously rejected short accelerometer/gyroscope vectors
before the original initialization, timestamp and sensor-source checks could
ignore them. It now retains a truncated-input tag through those checks; genuinely
admitted malformed vectors still error, and valid measurements keep their
existing numerical path. No validity threshold or missing-axis replacement was
introduced. [Issue #373](https://github.com/bin9208/openpilot-rust/issues/373)
tracks this defect and the separate camera/calibration admission follow-up.

Independent original captures cover 24 direct cases, 40 loop cases, six first
initialization cases and two recovery sequences. The initial native regression
failed two of three tests; all five sensor regression tests now pass, together
with the complete 17-test locationd package and strict all-target Clippy. A fresh
native daemon, private Msgq/Params namespace and unavailable executable PATH
also passed three successive real livePose publications after malformed,
rejected and valid input batches. The exact executable hash and source captures
are recorded in `rust/crates/locationd/tests/data/admission-373/README.md`.
Private logs are `2026-10-18-locationd-tools/sensor-green-final1-*` and
`sensor-live-final1/proof.json`. These tests establish admission and continued
host operation; full EKF numerical parity and device acceptance remain separate.

The camera/calibration follow-up also preserves original early rejection before
shape errors, NaN/bounds policy and late posenet history side effects. Its four
initial regressions failed before repair. Independent original captures include
34 admission cases and 29 edge cases; the complete locationd package now passes
23 tests and strict all-target Clippy. The fresh daemon additionally passed seven
real livePose publications through the private IPC probe, including rejected and
valid camera/calibration batches. Exact fixture and executable hashes are in the
same README; private evidence is `camera-calibration-green1-*` and
`camera-calibration-live1/proof.json`. No missing measurement is fabricated and
normal three-axis processing retains its existing numerical path.

No blanket module clearance, complete Python retirement, device boot acceptance,
loaded-driving acceptance or measured CPU improvement follows from this matrix.

2026-10-18 follow-up source review of crash-reporting and tombstoned found an
external-command ownership gap tracked in [#363](https://github.com/bin9208/openpilot-rust/issues/363).
The retrace loop reaps its shell with `try_wait()` before fallible output decoding;
error cleanup can then send a process-group signal using that already-reaped
leader's numeric ID. An exited leader with a descendant holding stdout reaches
the same ownership boundary. Actual owned-process traces reproduced a completed
`wait4` reap followed by `kill(-group, SIGKILL)`. The fix uses
`waitid(EXITED|NOHANG|NOWAIT)` and retains the leader until output decoding and
all required group cleanup are complete, then reaps exactly once. PID reuse,
unrelated signaling and device impact have not been observed. NativeSdk send/flush and tombstone copy/remove order had no additional
concrete finding in this bounded source review. This does not clear their
external transport, SDK teardown or arbitrary descendant behavior.

Docs-Not-Needed: internal engineering coverage record without setting changes.

## Generation ownership and native maintenance transactions (#287 / #290 / #297 / #309)

Persistent/bootstrap service ownership now uses the native executable and
process start identity, not an argument substring or executable basename.
Only a proven same-store old generation may be retired, through a pidfd with
bounded shutdown. Linux 4.9 lacks that safe signal boundary and requires normal
reboot; there is no raw-PID signal fallback. A live owner retains its entire
package generation so future sibling helpers remain available. Inaccessible
process inventory warns and defers cleanup without failing successful install.

Server/recovery maintenance and native updated finalization now prepare an
independent exact-SHA checkout plus its verified matching package before changing
the active source. Missing release, wrong origin or offroad failure preserves
the old pair. Pull requires fast-forward source; repair prepares the active SHA.
Native actions no longer reset/clean the active checkout or invoke SCons.
Startup uses a durable external journal and Linux atomic directory exchange;
unsupported filesystems fail closed. Completed rollback checkouts remain intact.
Only exact creator-format pending directories are reclaimed. Candidate/staging
must be real, separate directories; device/inode identity is rechecked before
exchange. Visible old-root processes block exchange except the current launcher.
Unreadable OS executable identities remain an explicit permission limitation:
normal reboot and the existing boot lock are required external boundaries.
Native updates cannot fall through the historical original-source overlay swap.

The packaged `rust/bin/openpilot-runtime-install` is required and must hash-match
the independently verified static standalone installer. #309 regressions first
accepted different sealed static/dynamic helpers and a self-consistent changed
standalone/receipt/sidecar, then passed after binding both identities. All six
package integration tests passed. Installer eight unit/43 CLI cases, full launcher
suites and strict all-target developer-tool/installer/launcher Clippy passed.
Earlier native recovery tests passed 21 cases plus four terminal tests; native
server tests preserve active source on package failure and retain actual error
codes instead of overwriting them with empty placeholders.

Original/native checks passed 20 Pull, 23 attempt, 15 unchanged service, 40 Tools
and 36 recovery scenarios plus one native cleanup case. Unchanged comparisons
remain exact; intentional staging/retention differences have explicit native
contracts and original-mutation/deletion negative controls. Five actual native
Tools HTTP success responses separately require staged/reboot flags while active
HEAD/source remain unchanged. Seven focused notification tests cover Korean,
English and Chinese staging, failed staging, same-SHA repair, requested reboot
and unchanged legacy messages; generated Tools assets are rebuilt. These are
host process/Git/archive/parser and browser-contract checks, not target boot,
package publication, device behavior or CPU savings. Original Python comparison
providers remain interim developer validation tools pending their own retirement.
Final source-matched target rebuild/seal and user device acceptance remain open.

## Logger packet allocation reuse

The non-conflated logger drain previously created a fresh Vec for every packet.
`MultiSubscriber::receive_one_into` now copies into caller-owned reusable storage
through the same atomic queue reader. Existing owned-return callers still use
the same reader and retain their API. Loggerd reuses one buffer for ordinary
packets; encoder packets retain independent owned allocations because the
encoder can queue them across segment rotation. Compression level, file writes,
flush/close behavior, drain limits, decimation and packet ordering are unchanged.

Two new queue-core tests initially failed because the reusable receive API did
not exist. After implementation they preserve allocation pointer/capacity over
128 variable-sized payloads and ring wraps, check owned payloads after further
publication, and cover empty/evicted clearing, conflation and overwrite reset.
The real mapped queued fixture additionally checks 1,000 receives without a
storage replacement, inactive/invalid subscriptions and existing reconnects.
Full msgq and loggerd package tests and strict all-target Clippy passed using
the coordinated cache. This demonstrates buffer reuse and host correctness;
it does not measure device CPU savings or establish target recording acceptance.

Focused source inspection of statsd found its ZMQ drain checks the stop flag on
every packet and retries EINTR; its metric flush remains dependent on reaching
the end of the incoming burst. No behavior was changed. The sensor worker-owner
audit separately found a surviving temperature worker can mask IRQ worker exit;
that defect is tracked in [#308](https://github.com/bin9208/openpilot-rust/issues/308).
An independent original-source execution with owned thread/sensor fixtures
confirmed that the inherited Python owner waits despite the IRQ worker already
being dead. Three deterministic native RED cases reproduced that wait for an
IRQ error, panic and unrequested successful exit, while the healthy two-worker
requested-stop negative control passed. The corrected owner retains its one
second supervision cadence, requests stop after any worker exits, joins every
worker even when logging fails, then attempts hardware shutdown before returning
failure to the manager. Six supervisor/cleanup tests plus the existing strict
sensor-settling test pass; strict all-target sensord Clippy passes. Builds used
the coordinated cache with 383 GiB free. No sensor thresholds, IRQ affinity,
hardware shutdown writes or normal requested-stop success were changed.
Device restart and sensor behavior remain unvalidated; issue #308 stays open.

The temperature warmup path also skips its ratekeeper while sensor validity is
false, matching the original Python `continue` before `keep_time`. This can spin
on I2C reads during initial settling. It remains unchanged: blindly moving the
ratekeeper would change first-valid publication timing at the strict greater
than 0.5-second validity boundary. Its timing contract needs separate evidence.

## Connectivity follow-up source inspection

The WebRTC HTTP owner in `runtime.rs` reaps completed connection tasks inside
its normal accept loop and bounds shutdown draining to 60 seconds; it does not
share the CarrotNavi completed-JoinSet retention pattern. This is source
inspection only, not a new WebRTC test or a review of peer/media internals.

BlueZ `agent.rs` reaps reply tasks during its receive loop. `client.rs` limits
scan ownership to one task and drains that task before accepting a new scan;
close signals scan/pair cancellation and joins the owned work. No equivalent
completed-task accumulation was found in those paths. Agent prompt timeout
cleanup, pairing/reconnect behavior and actual D-Bus behavior still need their
own focused review and validation.

2026-10-18 source follow-up inspected Wi-Fi manager task ownership, Athena
session/proxy ownership and Navd publisher ownership. Wi-Fi drains completed
operation tasks in its select loop; shutdown signals and joins initialization,
then aborts and joins operation and D-Bus tasks. Navd's publisher uses a bounded
one-command channel; its owner closes the sender and joins the worker on drop.
Athena reaps completed proxy threads on subsequent proxy starts and joins its
registered session/proxy workers at session shutdown. Follow-up actual-thread
regressions reproduced detached new workers when registry admission failed;
the scoped correction and evidence are recorded under #360 below. The Wi-Fi and
Navd observations remain source inspections only. These checks do not clear
network protocol, general error recovery, hardware or timing behavior.

Startup UI `network::start` installs one process-lifetime IP monitor behind a
static mutex, avoiding a new monitor on repeated starts. Its input owner bounds
the mouse-event queue to 140 entries and signals/joins its worker in Drop.
These are source-only ownership checks; UI rendering, native FFI, physical
input timing and connectivity behavior remain unvalidated here. The usbgpu
model worker's parent watcher is process-lifetime and exits on parent change;
model/USB execution internals remain outside this focused inspection.

Audio follow-up: soundd already checks PortAudio stream activity each owner
iteration and propagates inactivity/query errors. Beepd reaps completed alert
threads on updates; its explicit daemon-thread shutdown policy deliberately
returns without joining blocked alert workers. These focused source checks do
not clear audio assets, amplifier hardware, alert correctness or target timing.


## CarrotNavi completed connection ownership (#301)

The native owner previously added every HTTP/WebSocket connection to a JoinSet
and only drained entries during shutdown. A completed connection therefore
retained its task record for the full daemon lifetime. Before the correction,
the focused regression observed three retained tasks where only two were
expected after its second completed connection.

`Shared::spawn` now drains completed task records under its existing mutex
before adding a new connection. Pending connections remain owned for the
existing bounded shutdown drain. The deterministic current-thread regression
keeps one connection blocked on a oneshot while completing 128 others using
completion oneshots; the registry stays at two entries and both remaining
entries join successfully after release. No timing sleeps are used.

`cargo test --locked -p openpilot-carrot-navi --features native` passed all 19
tests, including embedded listener release and unresponsive-upgrade shutdown.
`cargo clippy --locked -p openpilot-carrot-navi --features native --all-targets
-- -D warnings` passed. The canonical coordinated cache, two jobs and disabled
incremental compilation were used; free space was 389 GiB before RED and
388 GiB before GREEN/Clippy. No protocol, radar lead selection or settings
behavior changed, and no device or CPU measurement is established by this fix.

## Native body teleoperation and obsolete source-presence gates (#303)

Tracking: [#303](https://github.com/bin9208/openpilot-rust/issues/303).
The current checkout omitted `tools/bodyteleop/web.py` and its browser assets;
its optional manager entry was therefore disabled and had no Rust candidate.
The complete original source and seven static assets were recovered from the
last pre-deletion tree `2256431333346686bfc27911b300dc6a9a07b166`. The assets
retain their original content with provenance in `rust/crates/webjoystick/assets`;
six files remain byte-identical; the final whitespace gate removed two trailing
spaces from blank `main.css` line 33 without changing CSS declarations.
the Python server is ported, not embedded or invoked.

The new native server retains HTTPS on all interfaces port 5000, original
index/static files, GET/HEAD ping, and the offer bridge to localhost:5001/stream
with driver camera, incoming testJoystick and outgoing carState. Startup writes
JoystickDebugMode before certificate setup as the original does. Existing PEM
identity is reused; missing certificate/key invokes the packaged native OpenSSL
provider with the original RSA-4096, 365-day and subject arguments. Invalid
existing identity, unavailable OpenSSL and failed/malformed backend responses
remain failures. Native TLS/HTTP connection tasks are owned, reaped and joined
on shutdown. The original NotCar manager predicate remains intact.

Manager availability now validates the real native binding, including ELF,
launcher, source root and OpenSSL provider, instead of testing `web.py` presence.
The USB cluster's Rust scanner and its installed binding no longer require the
unused Python vendor directory. Bus locking, native libusb initialization,
native display/encoder helpers and nonempty image/font assets remain required.
RED evidence: cluster binding failed on the absent vendor directory; the real
USB scan fixture failed because no bus lock was attempted; the initial native
HTTP route fixture returned 404 instead of the expected original index.

Four freshly compiled native web tests passed, including a real TLS handshake,
certificate reuse, pending-handshake shutdown, original asset bytes, offer
forwarding and backend errors, and the real CLI Params side effect before a
native certificate-provider failure. Full package and Clippy results will be
recorded after the coordinated build slot resumes. Normal device/body operation
and physical TURZX display discovery remain unvalidated. The optional service
must be included with native OpenSSL in the complete runtime package; disabling
it or retaining the old source-presence exception is not a completed port.

The nearby runtime audit found no additional Python process invocations in
manager installed bindings or cluster autorun. `python_str`/`python_float` names
are native compatibility functions. `manager/src/supported_cars.rs` still refers
to the separate build-time `tools/generate_manager_cars.py` export; conversion of
that developer-tool family remains tracked by the broader retirement work.

## Native encoder dependency staging

`openpilot-dev-tools` now provides `stage-encoder-target`. It retains both ARM
wheel filenames, URLs, git commits, exact archive sizes and SHA256 pins from the
former tool, checks unique `uv.lock` provenance, and extracts only native install
payloads. Selected ZIP contents are streamed with CRC/size checks into temporary
staging, then renamed after validation. Path escapes, links, duplicate names,
file/directory collisions, missing payloads and the 256 MiB limit fail closed.
A regression initially showed that zip 9's name index concealed duplicate
central-directory records; the native owner now checks the raw record count
before extraction. This is additional validation around the ZIP library.

The scaler uses the unchanged FFmpeg 7.1 SHA256/URL and requires byte-identical
avutil `version.h`/`pixfmt.h`. Bounded XZ/tar validation precedes extraction.
The unchanged configure arguments and `make -j2 libswscale/libswscale.a` are
recorded before execution. The archive, three public headers, LGPL-2.1 license
and LICENSE.md are copied and individually hashed in the receipt. Downloads
have explicit byte limits and local copies check size before copying; reserve
checks retain 25 GiB plus estimated growth before extraction, commands and copies.

Six native integration tests cover the original pin/extraction/unsafe-path/
source/header tests plus CRC, duplicates, missing payload, malformed ZIP,
oversized declarations, executable modes, unique lock rows and CLI failure.
Two unit tests cover source validation before writes and owned command/receipt/
license behavior, including a failed build command. Only a fake Runner executed
in the build-command test; no compiler, make, actual source build, target codec
or device validation was performed. The six integration tests and two focused
unit tests passed; all-target `openpilot-dev-tools` Clippy with `-D warnings`
passed. Available space was 387 GiB before each build. Existing Python staging
remains until its consumers are migrated and the real target build is verified.

#303 final host verification: full `openpilot-webjoystick`,
`openpilot-manager-catalog` and `openpilot-manager` package suites passed all 24
tests. Native body activation without Python source and unavailable native
OpenSSL are both covered by installed-manager inspection. All seven restored
assets were independently compared byte-for-byte against the exact original
Git objects. Strict all-targets Clippy for those three packages and
`openpilot-cluster` passed, as did the scoped diff check. Builds retained at
least 386 GiB free, used the coordinated cache and disabled incremental builds.

The full cluster suite's initial provider-complete enumeration reported 147
passes and four failures across `output_startup` and `software_h264`; those
fixtures hardcoded the absent `/usr/bin/ffmpeg`. The authorized fixture repair
accepts `FFMPEG_BINARY` while retaining that original default. Rerunning both
complete affected targets passed all five tests against the existing native
x86-64 FFmpeg ELF, preserving the real encoder subprocess and output assertions.
The canonical environment's `ffmpeg` PATH entry is a Python wrapper, so validation
used the native ELF inside the existing provider install directly. No system
binary, runtime selection or production encoder policy was changed. The earlier
four failures are environmental fixture assumptions, not hidden test exclusions.

One stale coordinated JPEG CMake cache still referenced the primary checkout's
source directory and initially rejected this worktree. Only its asserted stale
generated output directory was removed before rebuilding; no source or current
validation assets were removed. Complete target package/seal, source-matched CI,
physical body/WebRTC operation and actual TURZX device testing remain pending.

## Native static ELF audit

`runtime-elf-audit` and reusable `elf_audit::ElfInfo` replace the static audit
and shared readelf parser. Each root resolves only its own transitive DT_NEEDED
closure through the explicitly ordered library directories. Every discovered
object must be ELF64 little-endian with the requested machine (CLI AArch64).
Version requirements are compared against their selected provider's definitions;
strong imports must resolve within that root's closure. Default `@@` exports
also satisfy their bare symbol, while non-default version exports do not.
Weak undefined symbols retain their exemption.

Seven integration tests compile only tiny owned `gcc -nostdlib` shared-library
fixtures and inspect them with native readelf: matching versions, provider path
priority, wrong root/provider architecture, missing providers/versions/symbols,
isolation from unrelated root exports, weak imports, cycles and the CLI target
architecture gate. One parser unit test verifies default-version aliases and
weak exclusions. Parent's full developer-tool suite passed these tests, and
its strict Clippy/format gate owns final package-wide validation.

The built native tool additionally passed against four verified-image AGNOS
roots: the owned C probe, owned C++ probe, extracted OpenSSL and explicit loader.
Their individual closures contain 3/6/5/1 paths respectively, with zero failures;
11 object paths are inspected in total. The two explicit library locations
contain identical loader bytes under separate canonical paths. The complete
native report equals the original Python audit report for these four roots.
Local evidence is `.analysis/scratch/2026-10-14-agnos296/toolchain-abi.json`.
This is static architecture/dependency/version/symbol evidence for those roots,
not a full runtime inventory, whole-candidate ABI acceptance, device execution
or hardware validation.

## Sparse auto-exposure image copies (#296)

Source inspection found one full `layout.len` scratch allocation per enabled
camera in `RuntimeCamera::new`, reused but completely overwritten on every
publication solely for AE sampling. This is an existing persistent allocation,
not a demonstrated per-frame allocation leak. The original C++ sampler and
Rust reference both index `row * out_img_width + x`; they do not use NV12 stride.
That source behavior is retained explicitly, including padded layouts.

The compact candidate stores only selected owned bytes plus a fixed-size
checked iterator, without an offset-index allocation. It reads the same image
after the existing request fence completion and `VisionImage::publish` ION
cache synchronization. No new synchronization, mapped borrow or production
unsafe block is introduced. Existing atomic word and disjoint byte-tail
accessors supply owned byte copies, caching adjacent bytes from one word on
the stack. Full-copy APIs remain available for independent comparisons and raw
logging retains its original full-image path and decimation. Message encoding
still precedes AE update, preserving previous-command exposure metadata.

RED: the real padded VisionImage fixture, initially routed through the unchanged
full-copy implementation, rejected its compact 35-byte destination for a
360-byte image with `VisionIPC copy requires exact buffer length`. Geometry,
exposure, bounds, mapped tails and requested-allocation evidence are prepared;
final results and all sensor/camera buffer sizes follow after coordinated tests.
This candidate makes no measured target CPU, latency or DRAM-traffic claim.

## Independent native-tool review corrections (#305 / #306)

Publisher review reproduced a #305 gap: changing an installer during the first
read-only GitHub request caused the old implementation to trust the changed
bytes as its initial identity and publish them. Adding an unreviewed local asset
during final draft inspection also reached publication. Asset hashes are now
frozen before initial semantic validation, checked after that validation and
after read-only network work, and semantically revalidated against the same
identity immediately before public publication. Check-only follows the same
read-only validation. A separate regression reproduced a failed exact-SHA check
appearing during upload; protected CI evidence is now checked again before the
public PATCH. Remote resume and byte-download verification remain covered.
The unused unbounded stderr temporary file in native asset downloads is removed;
stdout retains its recorded-byte limit and failure/child cleanup behavior.

Sealer #306 review reproduced a malformed requirements object being silently
ignored while overwriting the manifest. Present requirements now must be an
array of objects with typed paths, known boolean kind flags and exactly one
active kind, matching the manager's emitted contract. Omitted/empty lists retain
their default behavior. Ten malformed schemas now fail without overwriting an
existing manifest.

Documentation-gate review additionally reproduced invalid placeholder waivers
hidden by U+001C..U+001F whitespace or the long-s spelling `<reaſon>`. Python's
strip/casefold rejected them, while the initial Rust gate accepted them. Native
trimming now includes Python's four C0 whitespace characters. For the fixed
invalid-token set, the extra long-s casefold mapping is explicit; a scan of all
Unicode characters confirmed this is the sole additional mapping into that set.
Regressions verify required settings documentation remains required for both
placeholder forms. A separate 7,800-case bracket-class comparison against Python
fnmatch found no differences in the reviewed matcher cases.

Observed RED covered both publisher asset races, the later CI change, malformed
requirements and both Unicode waiver forms. The corrected developer-tool suite
passed 56 tests (including eight publication, nine sealing and thirteen docs
checks), plus strict all-target Clippy. The parent's subsequently added disk
reserve overflow test separately reproduced an unchecked-add panic and remains
owned by the disk-space change; that later RED is not covered by the earlier
56-test success. Free space was 385–386 GiB before these builds. No remote
publication, actual CI authorization change or device action was performed by
this review.

Sparse AE GREEN: the full `openpilot-msgq`, `openpilot-camerad` and
`openpilot-camerad-runtime` package suites passed with runtime `native-skip-miri`;
strict all-targets Clippy for all three passed. The real padded-image allocator
fixture measured 360 requested bytes for the full scratch vector versus 35 for
its compact samples, and zero allocation during the warmed-up sample copy.
These counts describe owned scratch/output bytes, not allocator bookkeeping,
DMA, cache invalidation traffic or target CPU time. Full copying also reused its
scratch allocation; the change does not claim to eliminate a per-frame leak.

The nine sensor/camera comparisons use actual native NV12 allocation sizes and
source focal/region rules. Each scratch vector's capacity equals its listed
size. The same number of output bytes is copied per normal AE frame:

| Sensor | Camera | Output | NV12 stride | Previous scratch/copy bytes | Compact scratch/copy bytes |
| --- | --- | --- | ---: | ---: | ---: |
| AR0231 | Road | 1928 x 1208 | 2048 | 4,804,608 | 433,008 |
| AR0231 | Wide | 1928 x 1208 | 2048 | 4,804,608 | 229,336 |
| AR0231 | Driver | 1928 x 1208 | 2048 | 4,804,608 | 199,044 |
| OX03C10 | Road | 1928 x 1208 | 2048 | 4,804,608 | 433,008 |
| OX03C10 | Wide | 1928 x 1208 | 2048 | 4,804,608 | 229,336 |
| OX03C10 | Driver | 1928 x 1208 | 2048 | 4,804,608 | 199,044 |
| OS04C10 | Road | 1344 x 760 | 1408 | 2,428,928 | 234,490 |
| OS04C10 | Wide | 1344 x 760 | 1408 | 2,428,928 | 129,492 |
| OS04C10 | Driver | 1344 x 760 | 1408 | 2,428,928 | 106,602 |

For every combination, nine patterned frames produced bit-identical grey values,
register lists, serialized exposure state and Cereal camera-state bytes against
the existing full-copy sampler, including manual overrides and a disabled frame.
Exhaustive small/odd region, skip and input-length cases preserve the original
error acceptance and floor(total/2) descending histogram rule, including empty
and one-byte samples. Truncated compact input fails before Params access or
exposure mutation. Atomic prefix/partial-tail tests preserve frame-ID bytes;
an owned-image test verifies samples remain unchanged after producer reuse.
Existing raw logging, request/fence and frame synchronization tests also pass.
No native ABI/provider or target cache policy changed. Host builds retained
385 GiB free. Actual AGNOS ION/cache performance and device CPU benefit remain
unmeasured and require the complete source-matched candidate's device validation.

## Native media proofs and bounded QA commands

Four native CLIs now cover the dynamic-provider, graphics, FFmpeg and SDL proof
families. Their QEMU loader/library arguments, current-source symbol scans,
raylib contract/extension checks, three-codec encode/probe/decode cases and SDL
dummy-window pixel assertions remain explicit. FFmpeg and SDL static checks now
use the shared per-root ELF closure, so unrelated roots cannot supply exports.
SDL staging retains the exact wheel/source SHA256 pins and rejects duplicate,
linked, unsafe, oversized or CRC-invalid native ZIP entries before compiling.
The chosen SDL_ttf header must be unique, regular and bounded. Only native
providers, the font and the public header are selected.

Optional source pixel comparison uses `--source-oracle`: an independently
supplied executable receives the font path, writes original RGBA bytes to stdout
and version JSON to stderr. Its hash is recorded and must remain unchanged.
The project embeds no Python implementation for this optional original-source
boundary. Injected oracle fixtures prove equality is required and mismatches
fail; that test alone is not original Pygame execution evidence.

`qa_process` supplies bounded separate raw output capture, process deadlines and
owned process groups. Nonblocking pipes prevent output/pipe-holder deadlocks;
waitid NOWAIT retains the leader PID until group termination and direct-child
reaping. Cleanup sends SIGKILL to the owned group; descendant termination is
asynchronous and non-child descendants are not reaped by this owner. Five tests
cover stream limits, timeout/closed pipes, successful-exit background cleanup,
raw byte and nonzero/signal status handling. Bounded polling checks descendant
termination without assuming instantaneous signal delivery.

Seven media unit tests passed, including eight malformed SDL archive variants,
all ten FFmpeg commands, provider symbol groups, empty pixels and independent
oracle mismatch. The shared capture's five integration tests passed. Strict
all-target developer-tool Clippy passed after an independently owned OpenCV
module placement fix. Parent's runtime-bootstrap test executable was compiled
without execution as requested. These focused runs used the coordinated cache
with 384 GiB available before each build. An additional OpenCV preflight test
failed separately and remains owned by its port; this is not blanket tool-suite
clearance.

Actual native dynamic proof passed under AArch64 QEMU against verified-image
AGNOS libraries: 23 ELF paths and 42 current Rust-source symbols (PortAudio 10,
USB 20, RTMP 12), without audio/USB/network initialization. The native FFmpeg
proof passed against the existing verified release providers: 19 ELF paths and
all ten QEMU cases, including four complete decoded 96x64 RGBA frames for H.264,
VP8 and VP9. Evidence is under
`.analysis/scratch/2026-10-14-agnos296/{dynamic-native-proof,ffmpeg-native-proof}`.
Graphics also passed all four QEMU loader groups and 28 ELF paths: 117 raylib,
five EGL, 25 GLES and three extension symbols. SDL passed 14 ELF paths, an owned
dummy window and SDL_ttf 2.20.1 text rendering. All 9,880 RGBA bytes equal an
independent original Pygame 2.6.1 executable's output; both hashes are
`9a6350373163bb1f00b8051679bc045297e8b298d6ef90d539c343b04622128e`.
The oracle uses the official x86_64 wheel in ignored scratch (wheel SHA256
`1206125f14cae22c44565c9d333607f1d9f59487b1f1432945dfc809aeaa3e88`);
its source boundary, executable hash and versions are in
`sdl-native-proof-oracle/report.json`. The project's native validator contains
no embedded Python oracle.

SDL QA headers came from official SDL 2.28.4 source archive SHA256
`888b8c39f36ae2035d023d1b14ab0191eb1d26403c3cf4d4d5ede30e66a4942c`.
Its generic configuration replaces unavailable target-configured headers only
for this isolated probe compile; runtime libraries remain the exact pinned
aarch64 wheel and verified-image AGNOS providers. Cached Debian headers were
rejected because their configuration indirection only had an x86_64 target.
Disk checks retained at least 383 GiB before downloads, extraction and probe
compiles. Graphics/SDL reports are under the same scratch root in
`graphics-native-proof` and `sdl-native-proof-oracle`.

The four replaced media-validator scripts and their replaced shared ELF-audit
script were removed after these proofs. A hidden-file-inclusive source search
found no remaining executable callers; historical startup records retain their
original evidence chronology. Native commands are `validate-runtime-dynamic`,
`validate-runtime-graphics`, `validate-runtime-ffmpeg`, `validate-runtime-sdl`
and `runtime-elf-audit`, provided by `openpilot-dev-tools`.
No display/GPU, vehicle execution or complete candidate acceptance follows from
these checks.

Static ELF inspection now uses `qa_process` for each native readelf invocation:
30-second deadline, 16 MiB stdout and 1 MiB stderr limits, owned process-group
cleanup and explicit `LC_ALL=C`. PATH retains the configured native SDK tool;
no shell is used in production. Injected owned native tools test timeout,
excessive stdout/stderr and invalid UTF-8; a malformed actual ELF is rejected.
The two new tests failed against the original unbounded invocation, then all
three ELF unit tests and seven original ELF integration tests passed after the
change. Strict all-target developer-tool Clippy passed, with 382 GiB free.

For the startup cross-build, exact FFmpeg 6.1.1 source SHA256
`8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968`
was configured using the original minimal logger profile and actual AGNOS
target compiler. Only configuration probes and `make install-headers` ran;
no codec library was built. Public headers are avcodec 60.31.102, avformat
60.16.100 and avutil 58.29.100. Generated `avconfig.h` SHA256 is
`975611ad5eba15212d9e1d5fca9d4fdf0daec6d2269b2fcab8e29af8667164bc`.
Its little-endian/fast-unaligned flags match both target configure results and
actual compiler AArch64 predefines. Header receipts and hashes are under
`.analysis/scratch/2026-10-14-agnos296/ffmpeg611-headers/`.
The development prefix references the existing verified released libraries;
this is explicit external-provider reuse, not a new full runtime provider build.
It does not reuse host-configured headers or encoder FFmpeg 7.1 headers.

## Continued module audit: inspected scope and evidence boundaries (2026-10-14)

Five scoped audits inspected runtime/source paths at `0992e61f` and `828efb0d`.
Most evidence is **source-only**, including reading existing tests and original
implementations; those tests were not rerun by the audits. The table records
coverage, not whole-module clearance. Full source-clean candidate builds,
exact-SHA CI, normal device startup/recording/upload and user driving acceptance
remain separate gates.

| Area | Actually inspected | Explicit remaining coverage |
| --- | --- | --- |
| Telemetry/storage | uploader scan/cache/runtime and HTTP entry paths; web-upload streams/retry boundaries; dashcam manager/worker/pool ownership; statsd drain/publication; proclog Collector/cadence; journald child/poll/decode; bootlog snapshot/capture; tombstoned discovery/retrace; crash reporter runtime/policy/SDK entry; feedbackd; logmessaged main and logging producer/rate/communication; deleter retention/bookmark/lock paths | Complete HTTP/status/configuration/error matrix, dashcam reports/source extraction, log context/wire normalization, journal binary JSON, raw-file providers, external SDK/apport lifecycle, recording races and target kernel/filesystem/transport behavior |
| UI/connectivity/audio | startup-ui and UI framework/application run/stop/child/input/render boundaries; cluster frame loop/media/decode ownership; Bluetooth poll/task/BlueZ entry; Wi-Fi initialization/monitor/scanner/bus ownership; modem PPP/state loop; LPA AT/APDU/download boundaries; registration retry/identity; Athena session/proxy/forwarding loops; Jetlink owner/RPC bounds; navd publisher/resend; WebRTC HTTP/application maintenance; bridge polling; cweb-push request/stop; soundd PortAudio ownership; beepd workers; amplifier retries/register transactions | Full rendering/widgets/settings/actions, GPU/encoder imports, USB/display/audio hardware, D-Bus reconnection and credential epochs, AT/eSIM/ASN/HTTP contracts, Athena RPC/upload scheduler, ICE/DTLS/RTP/codec internals, protocol backpressure and all device timing |
| Vehicle/control/planning | card production/core steps and IO/Params polling with sampled Hyundai integration; CAN parser acceptance/counters/timeouts; radarcan batching/reset/staleness; radard input clock/publication; plannerd load/model/liveTracks triggers; controlsd runtime/control entry/publication; selfdrived runtime/settings ownership; CarrotMan actor/tick/bus; carrot-state compact envelopes; control-policy math/vehicle/PID; joystick/maneuver readiness; desire command freshness/geometry/helper entry | All platform-specific state/controllers/fingerprints, complete lead/scoring/filter corpus, lateral/longitudinal MPC/solver equivalence, all event/alert/HUD/suspend/desire branches, full route/source arbitration and complete numerical/control/CAN regression corpus. Shared radar changes retain their separate workflow obligations |
| Device/estimation | locationd runtime/invalidity/estimator/Kalman/FFI snapshot; calibration cache/worker/update/status/wire; paramsd workers/loop/Kalman and sampled wire; torqued lifecycle/history/buckets/message/SVD boundary; hardwared worker/write ownership and major loop; hardware-control native IO/display/reboot entry; hardware-info command/WPA/Tici entry; Ublox serial/Pigeon/framing; QCOM setup/serial/framing/assistance; Panda USB context/handle/transfer and SPI Linux kernel ownership | Generated Kalman/Jacobian/orientation math, rewind internals, full cache migrations/random fitting/providers, complete fan/power/thermal policy, GNSS decoders/ephemeris, QCOM measurement/wire math, receiver/modem/USB removal behavior, all native fault fixtures and hardware operation |
| Shared support | Params namespace/raw/bool/atomic write/short-read contracts; typed STRING conversion; Cereal schema build path; messaging configuration/segment ownership/frequency/liveness; log-format rotation/JSON/Unicode entry paths; runtime-core filters/float/procfs; monitoring policy/pose/events/packet; managed entry/typed failure/SIGINT ownership; checkout-status identity/capture/two-read debounce | Exhaustive syscall/power-loss/race coverage, generated bindings, adversarial wire/Unicode/float corpus, complete monitoring long-run states, crash SDK teardown, arbitrary external-command descendants and all target performance behavior |

### Concrete faults and failure paths

- **UBX split-preamble loss, #317:** the audit **executed the actual original
  UbxFramer class** on owned bytes. A valid `B562010700000819` frame preceded by
  noise succeeds in one chunk; splitting as `00B5` then `62010700000819` yields
  no frame because failed sync clears the trailing `B5`. Native
  `ublox/src/framing.rs:31-37` has the same branch. This is an inherited framing
  fault, not a proven Rust regression or observed vehicle outage. The subsequent
  [#317 repair](rust-ublox-framing-317.md) passed native RED/GREEN, full corrected
  original-decoder comparison, Pigeon/daemon and ASan/UBSan serial gates; the
  audit itself did not execute the native framer.
- **Packaged OpenSSL missing, #316:** parent separately identified the missing
  provider required by native webjoystick. Parent **executed provider validation**:
  verified raw-image pins, five-object ELF closure, target OpenSSL version under
  QEMU, RSA4096 certificate creation and host certificate verification. This is
  native provider/packaging evidence; it does not establish body teleoperation,
  on-device TLS startup or complete package acceptance.
- Athena's non-PC log worker originally retried errors before reaching its
  normal wait when the log directory is absent/unreadable. Source inspection
  confirms the original has the same no-backoff loop. The subsequent
  [#318 repair](rust-athena-log-retry-318.md) adds interruptible failure retry
  pacing and deterministic failure/healthy/stop coverage; target evidence remains separate.
- Wi-Fi initialization can keep awaiting the surviving scanner after its
  monitor errors, concealing initialization failure; raw D-Bus replies also
  have no explicit deadline and can retain the network scan lock. The [#319 repair](rust-wifi-worker-lifecycle-319.md) now propagates essential
  worker failure and drains siblings, with owned lifecycle and unchanged
  private-D-Bus comparisons. Generic method deadlines remain unresolved.
- QCOM setup trusts a firmware count (`FFFFFFFF` implies a 512 MiB mask), and
  delimiter-free diagnostic input can grow indefinitely. Original mask code also
  trusts this input. The [#320 repair](rust-qcom-diagnostic-bounds-320.md) now validates the
  complete 12-bit item-range table before any masks; valid original bytes and
  decoder/daemon comparisons pass. No huge pre-fix allocation was exercised.
  Delimiter-free envelope bounds remain open without a justified protocol cap.

### Specific storage/work candidates, without measured savings

- locationd previously read scalar time and gyro bias through full snapshots, copying
  18 state plus 324 covariance doubles (2,736 payload bytes) into two Rust Vecs
  per snapshot. The [#325 scalar-access change](rust-locationd-scalar-access-325.md)
  preserves current-state reads, ordering/reset/rewind and raw floating-point bits.
  Its measured rejection fixture drops from 400 to zero Rust allocations over
  100 events; one Eigen state-vector copy remains in the bias accessor. Native
  pre/post traces are byte-identical and the unchanged original comparison passes
  1,073 steps/457,096 values. This is not a device CPU measurement.
- CAN replaces latest raw bytes with a fresh clone on every accepted frame and
  allocates temporary signal values for each parse. Reuse must preserve failed
  checksum/counter publication state and counter side effects; shared radar
  dependencies require their established validation workflow.
- Torqued interpolation rebuilds time/value columns from its bounded history;
  QCOM assistance creates an 8 KiB Vec per bounded download read. Reusable
  storage needs interpolation/download/error/rename equivalence tests.
- Other inspected allocation sites: statsd materializes directory entries only
  to count; logmessaged copies single-part records through multipart flattening;
  Bluetooth/bridge recreate poll/readiness vectors; WebRTC snapshots session
  handles every 5 ms; proc-stat parsing collects temporary field vectors twice
  per process; float formatting builds a temporary digit String; JSON scalar
  accessors materialize child views for nonstrings; checkout polling builds a
  two-entry vector. These are source-backed candidates, not target measurements.
- Uploader allocates a 64 KiB xattr buffer for uncached queries. Its descriptor
  is absent from the actual catalog (also commented out originally), so this
  cannot be attributed to current device CPU. Preserve existing cache/marker
  behavior if optimizing the independently callable path.

### JSON scalar access without composite allocation (2026-10-18)

`JsonValue::text_eq` and `to_utf8` formerly called `view()` before rejecting a
nonstring value. Arrays and objects consequently allocated child-view vectors
and cloned document handles only to discard them. Both accessors now inspect
the stored node directly. String codepoint comparison, strict UTF-8 conversion,
lone-surrogate rejection and retained-child ownership remain unchanged.

The standalone probe links the actual log-format library and performs12,000
accessor calls on six pre-parsed scalar/composite values: allocation calls fell
from4,000 (RED) to0 (GREEN). It is required after workspace tests alongside the
existing parser allocation probe. Three log-format integration tests, strict
all-target Clippy and45 affected CI isolation/disk tests pass. Independent review
found no actionable issue. This demonstrates host allocation removal, not device
CPU or driving performance. Logs:2026-10-18-ci-diagnostic-routing/json-*.

### Inherited contracts deliberately retained

Original controlsd explicitly polls for 15 ms. Torqued intentionally calls its
state-mutating message/fit method for both publication and persistence on
overlapping frames; combining those calls changes numerical state. CAN empty
updates, proc metadata/smaps caching and unusual Swaglog startup retention order
also have original counterparts. Unbounded original external-command waits,
detached helper concerns and nonrewindable upload retry bodies were not promoted
to newly proven runtime faults without a failing owned fixture. Params supports
large binary values, and no arbitrary global cap was inferred. Ownership reuse
must not alias retained camera/message data, expose partial parse updates or
weaken validity, periods, priorities, control policies or settings.

Detailed inspected files and per-module exclusions remain in the five ignored
`2026-10-14-module-audit-{telemetry,ui-connectivity,controls,device,support}`
scratch notes. Neither those notes nor this consolidated scope establish
all-module functional/numerical clearance or measured CPU benefit.

### 2026-10-18 continued proc-stat allocation work

`ProcessStat::parse` collected all trailing stat fields into a growable Vec for
each process and collector identity recheck. It now reads the required first50
fields into a stack array, preserving minimum52 total fields, all parsed indexes,
ignored columns, extra-tail acceptance and owned comm. A standalone host probe
compiles the production parser unchanged and counts actual allocator calls:
1,000 records RED6,000 calls -> GREEN1,000 calls (the retained owned process name).
The probe is a required workspace CI step; extra/truncated/Unicode-whitespace
regressions were added. The complete runtime-core/proclogd package run passes
35 tests, including actual Linux sampling, PID-reuse and metadata ownership.
All-target strict Clippy passes for both packages. The final production-source
allocation probe still reports 1,000 calls. Logs are in the local
`2026-10-18-proc-stat-296` scratch directory. This is measured host allocation
work, not device CPU or drive data. Physical acceptance is user-owned: the user
confirmed no device is currently available and will test after pulling.

Exact-SHA fast CI on 55464c01 found the new 64 MiB probe reservation missing
from the disk-policy test's inventory (197 actual versus 196 expected). The
test now classifies and asserts that reservation separately, retaining all
179 original ordinary reservation arguments and their hash. All 13 disk guard
tests pass after the correction. This was a CI accounting failure, not an
allocation probe or runtime failure.

### Log collector and statistics directory allocation reuse (2026-10-18)

The log collector now moves the first received ZMQ frame into the record buffer
and appends subsequent frames. A normal single-frame record keeps its original
allocation; byte order, binary/empty records, level parsing, file emission and
message publication remain unchanged. Its pointer/capacity regression first
failed against the former flatten/collect expression and passes after the change.
Multipart and empty-record cases retain exact byte results.

Stats storage counts directory entries with a fallible fold instead of allocating
a Vec only to read its length. It still consumes the iterator through the first
error, checks the file limit before empty metrics and preserves publication order.
Full logmessaged/statsd package tests pass 27 cases, including existing storage
failure, full-directory and record formatting/rotation tests; strict all-target
Clippy and the actual collector build pass. Independent review found no actionable
issue. This is host allocation evidence, not a measured device CPU reduction.

The newly built collector also passes the unchanged real original/native ZMQ and
full-cereal comparison: 24 records across retention, rollover-open failure,
write failure, deep-object publication and the 61-second rotation boundary;
2,500-file retention and message queue capacities match. Idle SIGINT/SIGTERM
shutdown, empty-record rejection and invalid CLI paths pass. The pre-existing
explicit CPython recursion-depth difference remains recorded by the unchanged
reference suite. Results: local `2026-10-18-logmessage-reuse/native-peers/report.json`.

### qcomgpsd assistance download buffer reuse (2026-10-18)

The assistance downloader now retains its 8,192-byte Vec between response chunks,
clearing its length before each read. The prior implementation allocated/dropped a
new Vec for each chunk. Reads, writes, response-limit enforcement, retry/stop and
partial-file cleanup remain in the same order; no timing, URL or size policy changes.
The package's ten tests, all-target strict Clippy and actual binary build pass.
The unchanged original/native assistance comparison passes alternate-server,
HTTP404, oversized and retry scenarios. An additional 89,619-byte multi-chunk
comparison passes the same four scenarios and byte output/cleanup checks.
Independent source review found no issue. This reduces allocations by construction;
it is not a measured device CPU saving or device download validation.

### Uploader extended-attribute scratch buffer (2026-10-18)

Each uncached file formerly allocated a fresh 65,536-byte getxattr buffer and
returned that capacity even for a one-byte positive result (the cached clone
was already compact). XattrCache now reuses one read buffer and returns only the
actual value bytes. Missing attributes,
empty/short/long values, errors and cached values remain distinct; explicit
mark_uploaded invalidation and original stale-cache semantics are preserved.
One 64KiB scratch allocation is retained per cache after its first read in
exchange for removing repeated 64KiB allocation/zeroing on cache misses.
No file selection, upload status, retry or network policy is changed.

All 13 uploader package checks and strict all-target Clippy pass. The unchanged
original/native gate passes 212 filesystem scenarios, 10,000 backoff decisions,
32 local HTTP scenarios, RS256/ES256 signing and exact compressed payload bytes.
The actual uploader with original msgq/full cereal passes the daemon scenarios,
including upload requests, startup lock cleanup, logging, shutdown and startup
failures. Source-derived storage/allocation reduction is not a device CPU claim.
Local evidence: `2026-10-18-uploader-buffer/{gate.log,reference/report.json,daemon.log}`.

### Torque history interpolation scratch (2026-10-18)

Four interpolation calls per accepted pose formerly allocated separate time and
value vectors from the bounded 100-row history. Two 100-element stack arrays now
hold those columns, preserving logical VecDeque order and the unchanged NumPy
search/arithmetic implementation. The maximum added stack scratch is 1,600 bytes
per lookup; query/result allocation and history retention remain unchanged.

The dedicated allocation regression first observed 1,300 allocations over 100
poses and then passed at 500 after the change. It uses a wrapped warmed history,
checks exact returned points and reset behavior. All six torqued tests and strict
all-target Clippy pass. The unchanged original estimator comparison passes 3,613
steps / 298,537 packet fields / five expected errors; pre/post native traces are
byte-identical. The original loop comparison passes 2,250 steps / 1,293,144 packet
fields and 12 persistence steps / 101,564 persisted fields. Independent source
review found no actionable issue. No period, fit invocation, interpolation, lag,
state or control policy changed; device CPU savings remain unmeasured.
Evidence: `2026-10-18-torque-history/{red.log,green.log,baseline,native,loop}`.

### Journal tag allocation ownership (2026-10-18)

`parse_line` now transfers the decoded SYSLOG_IDENTIFIER byte buffer into its
published String after the complete JSON message and integer fields have been
processed. Previously it cloned that buffer. JSON ordering/content, absent/empty
tags, surrogate rejection, integer conversion and fatal-error order are unchanged.
No journal child, message validity or logging policy changes.

The first global-allocator probe was rejected by this crate's inherited unsafe
forbid and was discarded; that is not allocation evidence. The replacement safe
ownership test observed a missing-helper RED, then confirms pointer/capacity
identity and original text when the owned buffer moves. All seven journal package
tests, strict all-target Clippy and daemon build pass. Independent source review
found no actionable issue. The unchanged actual original/native IPC gate passes
155 messages and13 recoverable errors per implementation,19 fatal-input cases
and four child/signal lifecycle scenarios. No CPU saving or device result is claimed.
Evidence: local `2026-10-18-journal-tag/{red-owner,green,clippy,build,reference}.log`
and `reference/report.json`. The original comparator remains pending Python tooling.

## Upload ownership and allocation follow-up (2026-10-18)

Source inspection covered `uploader` file selection/xattr caching, dashcam worker
spawn/cancel/expiry/drop, and the web upload/WebDAV streaming readers. The dashcam
manager removes completed workers after output EOF, terminates and reaps owned
children on teardown, and the worker arms a parent-death signal before verifying
its parent identity. File transport reads bounded chunks; WebDAV detects an early
EOF and keeps cancellation/progress failures as upload failures. These are source
findings only, not fresh transport or device acceptance evidence.

The uploader's path-keyed xattr cache retains entries for the process lifetime,
matching the original implementation and its explicit cache tests. Revalidation
or eviction would change when external xattr changes become visible; no cache
policy was changed without that contract review. Multipart diagnostic uploads
also materialize their body in memory. These remain optimization candidates;
no measured CPU or memory reduction is claimed from this inspection.

### Multipart duplicate allocation removed (#351)

The subsequent bounded optimization appends each file directly into the retained
request body, removing the temporary full-file Vec and copy. The complete body
still exists before HTTP starts and remains rewindable for307/308 redirects;
file errors still occur before a request. No streaming/timeout/retry policy changed.

For a64MiB synthetic log sent to an owned loopback receiver, fresh child peak RSS
fell from138,096KiB to76,796KiB (repeat77,268KiB). The receiver checked every payload
byte, multipart headers/tail and Content-Length67,109,134; the payload hash and
response matched. This is a host allocation measurement, not device CPU savings.
All17 package tests, all-target strict Clippy and the unchanged99-case
original/native HTTP comparator pass, including multipart escaping, missing
settings, error status and redirects. Scope and exact-SHA CI are tracked in
[issue351](https://github.com/bin9208/openpilot-rust/issues/351). The Python
comparison tool remains pending under296.

## Bootlog snapshot xattr buffer reuse (#353)

[The focused #353 record](rust-bootlog-xattr-reuse-353.md) covers the synchronous
Params snapshot copier. One private lazy scratch pair now replaces repeated
65,536-byte name/value allocations across recursive copying. Actual filesystem
regressions retain both pointers/capacities over16files, two directories and
66attributes while preserving copied bytes, timestamps, modes, symlink behavior
and failed-snapshot retention. All nine package tests, strict all-target Clippy
and ten unchanged original/native process/filesystem comparisons pass locally.
No launcher, detached-worker, cleanup or ignored-errno policy changes. This
proves focused storage reuse and host behavior; device performance/boot remain
unvalidated, and the remaining bootlog internals are not cleared by this change.

## Declared CPU model inputs after graph pruning (#355)

The #296 strict driving prefix274 check now reaches a separate CPU artifact
signature defect: compilation completes, but the unchanged caller cannot write
`desire_pulse` because the native exporter registers only live input plans.
Original offline export retains all declared ONNX input bindings independently
of scheduled calls. This failure occurs before numerical execution, so it does
not establish prefix equivalence or a device failure. The focused repair must
preserve unused input names and physical capacities while retaining rejection of
unknown names, wrong byte lengths and undeclared live inputs.

The focused regression reproduced the missing binding before repair. Registering
all lowered input plans independently of scheduled liveness now passes both
offline-binding policies and both memory-planning policies, including F16
capacity and unchanged output after changing unused bytes. The missing live
declaration still fails without publishing a bundle. The unchanged actual
prefix274 comparison passes under LLVM18 and LLVM20 with both memory policies, including
declared input hashes, defined packed-half input lanes and exact full output.
Fresh12 terminal,24 producer and48 residual physical-oracle cases also pass
after the shared padding repair, as does scoped strict Clippy. Independent
provider-specific indexed captures also pass all24 complete caller comparisons
and12 physical programs on each provider. The final host gates below extend this focused coverage. Exact-commit CI is
still pending; issue #355 remains open. This is host evidence only.

The formerly rejected parallel depthwise producer contraction cases now pass
the unchanged48-case original comparison under both LLVM18 and LLVM20. The
implementation admits only complete, independently owned producer graphs;
24 captured ownership cases and144 rejection controls preserve intermediate
outputs and external-reader boundaries. These focused results do not establish
whole-model equivalence, A630 behavior, or measured device speedup.

Independent review also found a regression in the uncommitted Concat/Slice
optimization: a published whole-input slice could eliminate every CPU kernel,
so compilation rejected an otherwise valid graph. The same independent fixture
passes on committed `1dbc4860` and failed on the proposed optimization. The CPU
builder now retains a direct copy only for a published Slice alias of an Input
or Constant. Computed aliases and the empty-kernel rejection remain unchanged.
Sixteen input/constant, held/unheld and binding/memory combinations pass over two
frames, including original signed-zero and NaN payload bytes. Existing alias and
input-binding regressions and scoped Clippy pass; independent review found no
remaining blocker in that repair. The final host validation accounts for all 47 model-runtime integration targets,
the library and four binaries under LLVM18. The first broad run was deliberately
stopped at a completed target to repair the review finding; the remaining/affected
34 targets then passed 179 tests. This is cumulative coverage, not a successful
single uninterrupted command. LLVM20 separately passed all 15 required graph
targets (70 tests) and ten primitive/provider targets (16 tests). Strict all-target
Clippy and scoped formatting passed. A freshly built ARM helper produced the
original Add source and identical genuine A630 binary under QEMU. That one-kernel
proof does not establish GPU execution or whole-model compilation. Independent
review accepted the bound-output repair and mandatory LLVM20 CI routing. The
workflow retains full workspace/original-source checks and requires the pinned
LLVM20 package and library, with three additional disk guards; all 49 routing
and disk tests passed. Exact-commit CI and user device acceptance remain open.

## UI lifecycle pixel comparison on CI (#357)

At `1dbc48609d83aac36c42b154a48afd80f2650a86`, the
[UI/connectivity job](https://github.com/bin9208/openpilot-rust/actions/runs/39108803932/job/117620299293)
passes the Forms replay checks but fails two lifecycle reference comparisons:
`unknown-mode` and `profile`. Both rendered images have i64 pixel SHA
`adcf4f00ab1d7da275c23c2e8e001d0aea44feb44b2af4e3a102f1e752efd229`, while the
independent original capture requires
`cad51e8e6a7c57ca8b20faab3140d95be545ca5907cee031175a18aa7f481f31`.
Four other tests in that target pass. The failure differs from #354's original
GLFW connection failure. The job's renderer reports Mesa 25.2.8 with llvmpipe
LLVM20.1.2; this identifies the environment, not the cause. Failed test images
were held in a temporary directory and are absent from the uploaded artifact.
[#357](https://github.com/bin9208/openpilot-rust/issues/357) tracks diagnosis,
evidence retention and correction. Running the unchanged original and native
reference in the same local graphics context produces identical pixels. Changing
that context with Mesa's `nosse` setting changes the original raster too, while
both implementations still agree exactly. This demonstrates that the fixed
capture hash is not portable; it does not reproduce the exact CI hash or prove
the CI difference's cause.

The lifecycle regression now replays immutable original drawing commands through
the same pinned mechanical provider and compares every pixel exactly. All25
original cases,17PNG traces, source/helper/font hashes, measurement float bits,
state/exit/file/environment and recording checks remain bound to the independent
capture. Existing original fixture files are unchanged. Mutation controls reject
changed calls, order, fonts, measurements, cases, mechanical sources and pixels.
Failures retain both images, state, command context and process output before
temporary-directory cleanup.

The first real replay gate exposed an adapter error: exporting while a texture
target was active incorrectly opened an idle window frame. Both recording cases
failed the raw protocol's duplicate-frame guard. Tracking texture scope fixes
that adapter; deliberate duplicate `begin` still rejects, including when the
helper exits zero. All17 original traces then passed strict protocol and exact
pixel replay. The full lifecycle target passed14tests, the five-mode test passed
again under `nosse`, and scoped strict Clippy passed. Independent review found no
remaining blocker. The five affected shared-adapter suites then passed all 52
host tests: network 8, render 9, Forms 14, family text 11 and admission 10. All
providers and test/tool ELFs were frozen and hash-checked before and after the
real graphics runs; required providers were present, with no optional early exit.
Scoped strict Clippy also passed for these consumers. Evidence is under
`.analysis/scratch/2026-10-18-ui-lifecycle-pixel-357/impacted-eRtBSKAB`.
Exact-commit CI remains pending. No device or full CI acceptance follows from
these host checks.

### Athena proxy workers remain owned on registration errors (#360)

The proxy start path previously spawned its bridge thread before locking the
registry and reaping previously finished workers. A poisoned registry or a
previous worker panic returned an error and dropped the new JoinHandle, leaving
that live worker outside session-owned shutdown joins. A mechanical extraction
of the existing ordering enabled actual-thread controls; both error controls
observed the rejected worker start and failed, while the healthy ownership
control passed. Each failing control released and observed its worker's drop.

Registry locking and completed-worker checks now precede spawning. The new
handle is inserted while the same registry guard remains held. Already pending
workers remain registered, completed workers are reaped, and admission failures
drop the unstarted closure and its connection resources. Network setup order,
bridge protocol, poll cadence, priorities and stop behavior remain unchanged.
All seven Athena library tests and all-target tests pass, followed by strict
all-target Clippy. Evidence: `.analysis/scratch/2026-10-18-athena-proxy-296/`.
Independent source/test review found no blocker. Exact committed-SHA CI remains
required; this is not a whole-module clearance, device result or measured
performance improvement.

### Application diagnostic window guard and exact source pins (#354)

The `4ed93061` UI/connectivity job stopped before graphics comparisons because
`build-ui-application-reference` still required the old shared Forms dispatcher
hash after the failed-window guard landed. Its exact source pin is now updated.
Review also found the independent `app-window` command had its own unguarded
`InitWindow` call: an actual raylib test with an absent display reproduced signal
termination after subsequent font work. The same readiness check now rejects
that command before setting window ownership; font work reports the existing
missing-window error and `quit` succeeds.

The original draw corpus and source/environment/provenance captures are unchanged.
Its mechanical boundary check requires three specifically reviewed current hashes
(shared guard, application guard and builder pin), with the fourth X11 pin unchanged.
It does not accept the old unguarded versions. Successful drawing still uses the
same calls, arguments, state/error checks and exact pixel comparisons.

With the real pinned raylib provider and an owned authenticated Xvfb `-noreset`
server, all four raw-boundary and fourteen application reference tests passed.
This includes the failed-display regression, actual XTest input, five original
modes, twenty environment cases and independent draw replay. Strict scoped Clippy
passed. Evidence is under `.analysis/scratch/2026-10-18-ui-application-window/`;
independent review found no blocker. This fixes the observed build-pin failure
and separate application diagnostic crash; the earlier intermittent display-open
cause and user device startup remain unverified.

### Uploader cache follows observed file deletion (#361)

Both the original Python xattr helper and native uploader retained historical
path entries after log deletion. An unchanged-original probe read and removed
512 actual temporary files and still found 512 cached entries. The native
regressions likewise failed after deleting whole segments and individual files.
This is an inherited retention problem, not evidence of a new port regression.

The uploader now reuses its completed directory inventories to remove cache keys
whose absence is established. Live files keep their existing cached values,
including external attribute changes; locked or unenumerated directories,
incomplete root scans, uncertain entries and other roots remain conservative.
There are no additional per-cache-entry filesystem calls. Substantially oversized
map storage is released after contraction. A file observed absent and later
recreated is read afresh: this is an intentional stale-cache correction, recorded
separately from unchanged source behavior.

All sixteen uploader tests passed, including three new real-filesystem regressions
covering three generations of 256 entries, capacity contraction, deletion/recreation,
locked/uncertain roots and preservation of other-root/live entries. Strict all-target
Clippy passed. A freshly built, frozen native trace binary also passed the original
212 filesystem scenarios, 10,000 backoff decisions and 32 loopback HTTP cases;
compressed payloads remain byte-identical and RSA/EC signing checks passed.
Independent source review found no blocker. Real enumeration errors were not
injected by the new tests; source inspection verifies failed inventories do not
prune. Evidence is under `.analysis/scratch/2026-10-18-uploader-cache-review/`.

These checks establish cache entry/capacity reclamation on observed deletion,
not measured heap bytes, CPU savings, a hard bound during indefinitely unreadable
directories or device upload acceptance. Exact-SHA CI and user device checks remain
separate delivery evidence.

### Updater process-group ownership follow-up (#364)

The related source review found `updated::process::NativeCommands::run` reaping
its command leader with `try_wait` before a numeric process-group signal, followed
by another unconditional signal in `GroupChild::drop`. The native updater QA
checker's `Probe::capture`/`OwnedChild::drop` also needs this ordering corrected;
its separately reported worker-group cleanup requires its own ownership review.
Tracked in [#364](https://github.com/bin9208/openpilot-rust/issues/364). This is
source evidence pending actual synthetic reproduction and repair. PID reuse,
unrelated-process signaling and device impact have not been observed. The examined
cluster font inventory and recovery reboot owners already retain their leader
with `WNOWAIT`; they are not included in this finding.

### Tombstone retrace ownership repair evidence (#363)

Both actual regressions fail before the fix: invalid UTF8 output, and cancellation
after an exited leader leaves a descendant holding stdout. The cancellation
barrier waits for the owner's actual exit observation, preventing an early-stop
false positive. After the fix, syscall traces show `WNOWAIT`, the final group
signal, then exactly one reap; UnicodeDecodeError remains identical to the original.
The successful path decodes and normalizes output before reaping. Filename argv,
inherited streams, result/exit mapping and the default30-second deadline are unchanged.

All ten package test entries pass (including the two owner regressions and their
reexec dispatcher), as do all-target strict Clippy and fresh daemon/example builds.
The newly sealed binaries pass all242 unchanged original comparisons including
the30-second timeout, literal-filename safety and native SIGINT/SIGTERM held-command
cleanup. The actual shutdown cases retain the crash, do not copy/upload it, leave
no active owned descendants and exit0. The old original SIGINT control's known
active descendant is still recorded and cleaned by the unchanged validator.
Existing workspace CI already installs mandatory strace and runs the package;
no test skip or tolerance was added. A separate full live-daemon/collector gate
was not inferred from an unverified cached logmessaged binary.

Evidence: `.analysis/scratch/2026-10-18-tombstone-owner-review/FINAL-EVIDENCE.md`,
`before-after-proof.json`, `native-regression-CYhxdR49/{red,green,clippy,build}.log`
and `affected-original/receipt.json`. Parent reviewed the exact source diff,
real pre/post syscall traces and source-bound validation receipts. #364 updater
ownership work is separate and remains pending. Exact committed-SHA CI and user
device acceptance remain open.

### Updater process ownership repair evidence (#364, 2026-10-18)

Actual runtime success, nonzero and invalid-UTF8 regressions reproduced the
pre-fix reap-before-group-signal ordering. The native checker independently
reproduced three Probe cases and a late signal using a previously reported
worker group. No PID reuse or unrelated-process impact was observed.

The runtime and checker now observe exit with WNOWAIT, perform final group
cleanup while the leader remains waitable, reap once and disable subsequent
Drop signals. The source-pinned fixture's single child worker is verified against
its still-owned probe's parent/group/session/start identities before acquiring a
pidfd. Worker cleanup uses that pidfd, never a stale numeric group. A malformed
report is rejected without signalling its named process; the owned probe receives
its normal interrupt and four-second cleanup budget.

All20 updater package test entries, nine actual checker ownership entries and six
existing checker tests pass, with strict Clippy for both packages. The added active
worker regression proves pidfd termination while the worker is still running;
normal completion alone would not prove that path. Wrong-parent/wrong-group
fixtures remain alive after rejection. A freshly built/frozen native checker and
updater example pass all five unchanged original baseline cases (seven report
rows), with23 native execs and no Python execution. Merged output/newline behavior,
nonzero status, Git maintenance environment and both SIGINT leader states remain
unchanged. The mandatory CI gate supplies the freshly built process-child launcher
and installs strace; no absence-based skip is permitted. Exact-SHA CI is pending.

Evidence: `.analysis/scratch/2026-10-18-updated-owner-review/FINAL-EVIDENCE.md`,
`native-gate-mp9BcUdL/{runtime-red,checker-red,runtime-green,checker-final-green}.log`,
`actual-current/receipt.json` and `final-sources.sha256`. The original baseline hash
remains `b7a2ed108a17d2010516112b20e491493aace5c26d43bf4d84a135bacc2be752`.

### Git-status command ownership repair evidence (#365, 2026-10-18)

Actual cancellation and eight-second timeout cases reproduced Git-status leader
reaping before final TERM/KILL signals when a child retained stdout. Cancellation
waits for the owner's real exit-observation syscall, so an early signal cannot
produce a false pass. WNOWAIT now retains the leader through pipe draining and
cancellation cleanup; the final wait disables further group signals. The original
TERM/grace/KILL policy and output/status handling remain unchanged.

Both actual syscall regressions pass, checking final-signal-before-single-reap,
no active owned descendant and repository lock release. All24 library entries,
five actual staged-update scenarios and strict all-target Clippy pass. Freshly
built/frozen Git-status and process-child executables also pass all27 unchanged
original scenarios, including Git/fetch timeouts and exited-leader-held-output,
and both optional queued/active polling cancellation scenarios. Existing0.6-second
source/native cleanup tolerance is unchanged. This verifies29 differential
scenarios, not CPU savings or device acceptance. Exact-SHA CI remains pending.

Evidence: `.analysis/scratch/2026-10-18-git-status-owner-review/owner-red.log`,
`owner-green.log`, `clippy-green.log`, `green.sha256`,
`full-original-green/result.json` and `poll-original-green/result.json`.

### Startup CI retained-space repair (#367, 2026-10-18)

At `518f1657`, startup prerequisites passed its checkout-tool tests but the next
disk guard correctly stopped work: free space was24,111,370,240 bytes versus the
required27,111,981,056 bytes. The initial build disabled incremental compilation,
but subsequent test steps used default incremental/debug profiles. The job now
sets incremental=0 and development/test debug=0 for every Cargo invocation,
matching existing bounded jobs. The25 GiB reserve and all original growth guards
remain unchanged. No disk exhaustion was observed and no test was removed.

The new native workflow assertion failed on the missing job environment before
repair. All49 routing/space tests now pass, including the #364 mandatory ownership
checks. There are251 explicit space guards across30 jobs; all30 now include a
non-renderer guard. The original177 guard argument hash is unchanged. Full new-SHA
cloud execution is still required to establish available runner space; a host
contract check is not a measured cloud disk result. Exact failing job:
https://github.com/bin9208/openpilot-rust/actions/runs/39127259281/job/117674947599.
Evidence: `.analysis/scratch/2026-10-18-startup-ci-space/contract-red.log` and
`contract-final-green.log`. Parent independently reviewed #364 source, its actual
syscall evidence and native baseline receipt; a separate source review found no
blocker in #365. Device startup/recording/upload remain user-owned.

### Missing metadata diagnostic transport lifetime (#366, 2026-10-18)

Exact518f1657 telemetry CI lost the missing-build-metadata log although the native
console emitted its original message. The metadata helper created and dropped a
new PUSH socket on each error. An independent unchanged Python SocketHandler
control retained its queued record after emit returned and delivered it when the
collector bound later. Both native regressions failed before repair: one main
caller and three short-lived emitting threads, with the original2-second receive
deadline. This establishes transport lifetime loss, not missing error generation.

The helper now uses a process-retained Python-compatible producer. Only Factory
and socket connection state enter its mutex; ordinary Rc-backed Logger remains
neither Send nor Sync. Formatting stays on the caller's thread. The shared
connection implementation preserves nonblocking send, EINTR/EAGAIN behavior,
10ms linger, inherited-PID reconnect and closed-socket errors. No sender thread,
format replacement, production unsafe block or receive-timeout increase was added.
An outermost-only RuntimeLogScope closes the connection after final diagnostics;
unused guards initialize nothing. Managed entry, manager, launcher, Athena and
relevant stream fixtures retain that guard. This is not a guarantee of delivery
when no collector becomes available before process shutdown, nor arbitrary
multithreaded-fork safety.

Validation passed:17 existing logging tests, six isolated runtime modes, two
compile-fail thread-contract docs and both actual metadata lifetime regressions;
strict all-target logging/version Clippy; affected executable/example boundary
checks; and all50 workflow routing/space tests. The active scope-drop case proves
Drop closes the socket before any explicit close. Original comparison with fresh,
frozen executables passed all171 metadata/Git cases with zero divergences,2472
exact record packets,20 event packets and four source errors, ordinary/native
thread contexts, backpressure, console failure, fork and original/native collector
paths. Actual metadata collection produced one logMessage, one errorLogMessage
and one disk record with unchanged error content. Causal connect interruption and
invalid-endpoint controls also passed for both original and native producers.

The telemetry CI job now runs both lifetime tests mandatorily. Its and the startup
service job's new native checks use bounded incremental/debug profiles;253 guards
cover31 jobs and the original guard-argument hash is unchanged. Exact committed-SHA
CI remains required. Device startup, recording and upload are still user-owned.
Evidence: `.analysis/scratch/2026-10-18-version-log-ci/`, including
`native-red.log`, `original-control/receipt.json`,
`producer-gate-9KcbHRBA/FINAL-EVIDENCE.md`, `boundaries-check.log`,
`ci-contracts-green.log`, `reference-green/manifest.json`,
`producer-original-green/report.json`, `collector-green/manifest.json` and
`connect-original-green/result.json`. Separate read-only boundary review found no
blocker. Other short-lived helper logger sites are explicitly inventoried in
`boundary-and-ephemeral-review.md`; packet loss there is not inferred as tested.

### Dashcam metadata Git pipe capture (#369, 2026-10-18)

Actual public `metadata::git_text` calls reproduced three failures: a successful
262144-byte stdout or stderr producer returned the fallback after filling its
pipe, and a leader that exited while a private descendant held its pipes escaped
the original four-second deadline. The old loop waited for exit before reading
either pipe and then used unbounded `wait_with_output`. Four controls (invalid
stdout/stderr UTF-8, nonzero exit and empty output) already passed.

The helper now reuses process-supervision's bounded concurrent capture, preserving
the four-second total deadline, status/UTF-8 validation, whitespace handling and
fallback. It terminates/reaps its direct child on capture failure; it does not
introduce process-group signaling or claim ownership of arbitrary descendants.
The regression's outer group owner cleans up its private fixtures while retaining
its own direct leader until final cleanup.

All seven native cases pass after repair. The unchanged original `git_text`
function, AST-extracted with its actual subprocess call, independently passes the
same seven executable fixtures; its held-pipe case returns fallback at4.004s.
The existing full original metadata comparison passes3184 cases with zero
mismatches. Dashcam-upload and process-supervision all-target tests, dashcam strict
all-target Clippy and51 workflow routing/space checks pass. Mandatory support CI
now runs the new regression;254 guards retain the original177 argument hash.
Exact committed-SHA CI remains required, and physical upload acceptance belongs
to the user. Evidence: `.analysis/scratch/2026-10-18-dashcam-git-capture/`, including
`native-red.log`, `native-green.log`, `original-control.json`, `affected-tests.log`,
`clippy-final.log`, `ci-contracts.log` and `original-metadata-green/report.json`.
An independent source review found no blocker; its bounded direct-child scope and
test-only group ownership are recorded in `sol-image-review.md` there.

### Encoder fixture isolation and startup CI component (#370/#371, 2026-10-18)

Exact ad450fa6 Rust CI run39130502778 reproduced `ETXTBSY` in the encoder
checker's owned executable-copy identity test. The real mutation/hash assertions
now run in an isolated re-execution of the immutable test executable, avoiding
unrelated parallel-test fork inheritance of an open writer. The parent requires
both successful exit and one executed test. No codec behavior, hash comparison,
error retry or acceptance rule changes. All eight target-checker tests and strict
scoped Clippy pass locally.

The same run's startup-services job passed its updater ownership tests but then
failed because the minimal Rust toolchain lacked cargo-clippy. That step now
installs the pinned1.94.0 Clippy component before invoking it; workflow regression
coverage requires this ordering. All51 routing/disk tests pass. The existing254
disk guards remain. The separate telemetry and startup-prerequisites jobs passed
on ad450fa6, including the new metadata transport and checkout disk-profile checks.
These results do not establish a successful complete run or device acceptance.
The new committed SHA still requires cloud validation. Local logs and downloaded
failing job logs: `.analysis/scratch/2026-10-18-ad450-ci/`.

### Upload and bookmark owner follow-up (source inspection, 2026-10-18)

Inspected uploader runtime/HTTP/scanner, web-upload folder and WebDAV transfer/
stream/transport, and feedbackd's main loop. Uploader's logger is retained by its
runtime owner across scan/upload iterations; its public CLI rejects zero cycle
counts. Feedbackd likewise retains its logger and publisher across the bookmark
loop. These sites differ from the reproduced short-lived helper loss in #368;
no new lifetime defect was established in this inspection.

WebDAV streams bounded64KiB reads, checks cancellation between reads/operations,
verifies sent length and unchanged file metadata before MOVE, and tracks temporary
upload cleanup with a local owner. Folder uploads keep their separate original
chunk/retry/remote-size policy. Uploader compressed bodies still allocate the full
compressed file, matching its current implementation; streaming that path would
require preserving Content-Length, seek/replay and redirect behavior. No speculative
change was made. This source review does not validate remote-service behavior,
all HTTP edge cases, physical upload acceptance or measured memory/CPU savings.

### Retained helper Logger facade (#368, 2026-10-18)

Real manager default-parameter repair and QCOM assistance injection each lost
their warning/success record after the helper or short-lived emitting thread
returned, before a delayed collector connected. All four cases reproduced loss
with the unchanged2s receive deadline. Independently executed original manager
all-key default-setting loop and original QCOM injection (including its real retry
decorator and an owned successful mmcli fixture) delivered the records in both
modes. Manager still repaired the invalid INT to3 and emitted no duplicate on
its second pass. These are private host operations, not modem/device tests.

An opt-in `Logger::for_runtime_retained` keeps the existing concrete caller-local
Logger/context API while sending its already formatted packets through the same
process-retained Connection as `emit_runtime`. No new sender thread/socket/global
is introduced. Closing or dropping a facade cannot close another facade's
transport; only the process-root RuntimeLogScope/close_runtime closes the shared
connection. Ordinary Logger ownership and nonblocking/error/fork policies remain.
Only the two reproduced production helper constructors changed.

Nine real ZMQ facade modes,17 existing integration tests, six existing runtime
producer modes and two compile-fail documentation tests pass. Both metadata modes
and all four actual helper modes pass; strict all-target Clippy for logging,
runtime-version, manager and qcomgpsd passes. A fixture's initially16-character
thread name exceeded Linux comm's15-byte limit; the same shorter operation-log
name was independently recaptured in both originals and native tests. Production
metadata and assertion deadlines were not changed to hide this fixture error.
Mandatory telemetry/platform/GNSS CI now runs these tests;51 routing/disk tests
pass with257 guards across32 jobs and unchanged original guard arguments.
Exact committed-SHA cloud checks remain required. Evidence and source hashes:
`.analysis/scratch/2026-10-18-ephemeral-runtime-logs/facade-green-JhCubJnh/`;
original controls in sibling `original-manager-qcr9pgwn` and
`original-qcom-al1tpngb`. Two Carrot Server helper candidates remain unverified
and unchanged pending actual operation reproduction.
Independent source review found no blocker in facade ownership, the two actual
callers, regression cleanup or required CI registration; review source pins are
in `sol-model-facade-review.md` beside that evidence.

### Pinned zlib SDK download endpoint (#372, 2026-10-18)

The3bc673dd UI/connectivity job117692433833 rejected an11,974-byte response from
the zlib fossils endpoint against the existing pinned source SHA256. The failed
body was not retained, so its contents and cause are unproved. Fresh host downloads
from that URL and the official madler/zlib GitHub v1.3 release were byte-identical
1,495,873-byte archives with the unchanged required digest
ff0ba4c292013dbc27530b3a81e1f9a813cd39de01ca5e0f8bf355702efa593e.
Only the manifest URL changes to that official release asset. Version, archive,
source bytes, receipts and both checksum validations remain unchanged. Exact new
SHA remote SDK staging/UI checks remain required; a successful local download is
not a successful UI build. Evidence: `.analysis/scratch/2026-10-18-zlib-source/` and
`2026-10-18-ad450-ci/3bc-ui-connectivity.log`.

### Carrot Server request-local diagnostic loss (#368, 2026-10-18)

Actual public upload-health and vision-diagnostic HTTP requests reproduced the
remaining helper lifetime fault in all four success/upstream-failure modes. Each
request returned its expected response and emitted the invalid CarName warning
to stderr, but the collector connecting after request/application/Tokio-worker
completion received nothing within the unchanged2s deadline. Independent original
HTTP controls delivered the warnings in all four cases. These controls use owned
loopback endpoints and private Params; the original vision snapshot is explicitly
a fixture, so this does not claim whole-snapshot or remote-service equivalence.

Only diagnostic_metadata and dashcam/upload_health now construct the existing
retained Logger facade. The process-root managed-entry scope already owns shutdown;
no request-local shutdown, worker thread or socket was added. All four real native
HTTP cases now deliver exactly one warning with the original message, native helper
callsite and correct process identity after request completion. Payload/fallback,
health500, vision502 and successful200 responses remain asserted. No unreachable
webhook Params keys were registered or changed to manufacture another warning.

All24 Carrot Server library tests, five policy tests and strict all-target Clippy
pass. The new custom integration target is registered without feature/environment
skips and runs through the required workspace test gate. Independent source review
found no blocker in the constructors, owner boundaries or regression. Evidence:
`.analysis/scratch/2026-10-18-carrot-log-lifetime/` contains native-red-1.log,
native-green-1.log, affected-tests.log, affected-clippy.log and source-review.md;
original-http-ip_nnes3/receipt.json preserves the unchanged original controls.
Exact committed-SHA cloud validation remains required; the user owns device tests.

### Athena context-binding diagnostic lifetime (#368, 2026-10-18)

The public Athena context-binding operation reproduced the same short-lived
logger loss on both the main thread and a joined worker. A private malformed
DongleId produced the expected null fallback, metadata and stderr warning, but
the collector connecting after binding completed received no record within2s.
Independent unchanged manage_athenad context-binding controls delivered both
warnings with the existing global context. Those controls use real private
Params/build metadata and an explicit PC hardware value; they do not execute the
supervisor loop, remote network or vehicle.

Factory::runtime_logger now creates the existing retained facade from the
caller's factory, preserving already-bound context. Logger::for_runtime_retained
delegates to it. Only Athena logging::bind changes its constructor; unverified
worker-exception candidates remain unchanged. Ordinary factory loggers retain
their existing ownership, and no additional sender/socket/thread is introduced.

Both public-operation regressions now pass with exact warning, native callsite,
PID, preexisting context, metadata fallback and no duplicate. All Athena/logging
tests pass:26 ordinary assertions including the two compile-fail documentation
tests, plus17 custom lifetime modes. The existing ignored context_child is an
owned subprocess entrypoint explicitly executed by its two parent tests, not
an omitted validation. Strict all-target Clippy passes. The new integration
target runs through the workspace gate without an optional feature or skip.
Evidence is in `.analysis/scratch/2026-10-18-short-red/athena-*` and
`2026-10-18-athena-log-lifetime/original-pb89fib6/receipt.json`.
Exact committed-SHA CI and user device acceptance remain separate gates.

## Upload file consistency follow-up (#388 / #389)

A bounded review inspected uploader transport/source opening, completion marking,
folder stream completion, WebDAV consistency checks and cancellation/retry cleanup.
It identified two inherited defects; neither is an observed route incident.

Uploader marks `user.upload` by pathname after transport completes. A real-file
control of the unchanged original method consumed inode A, renamed it, created B
at the old path and returned a controlled success. The original marked B even
though only A was consumed. The current Rust transport/mark boundary has the same
missing identity binding. [#388](https://github.com/bin9208/openpilot-rust/issues/388)
requires a native regression and completion tied to the transferred file, while
preserving existing status and size policies. This original control uses a
controlled transport response; it is not a full HTTP or native-daemon test.

Ordinary folder upload also accepts only a matching byte count. An actual original
Python upload to a private loopback HTTP receiver sent a 2 MiB file; after its
first 1 MiB progress notification the entire file was rewritten at identical
length. The receiver enforced the declared length, received an A-prefix/B-suffix,
and the original reported success despite matching neither complete source
version. Rust FolderUpload lacks the consistency checks already present in its
WebDAV sibling. [#389](https://github.com/bin9208/openpilot-rust/issues/389) tracks
a native reproduction and an explicitly stronger changed-source contract.
Both defects are now reproduced by native integration regressions. Uploader's
actual HTTP acknowledgement and ignored-response cases both mark the replacement
inode incorrectly; its unchanged-file control passes. FolderUpload's private
HTTP receiver actually publishes the mixed same-length body and accepts a source
pathname replacement. Append and truncate controls already reject their attempts.
These are two failing uploader tests and two failing folder tests, not merely
source inspection. A further real-file regression reproduced a same-inode rewrite
inside the success-log callback, after the first metadata check but before marking.

Uploader now retains the actual body descriptor and its metadata through raw and
compressed transfers, checks identity before accepting completion, and marks via
`fsetxattr`. A final held-file check after success logging catches that callback
boundary; a detected change restores the previous last filename, emits failure
and leaves the marker unset. The already emitted success event cannot be
retracted, so this late-change case has success followed by corrective failure.
Zero/oversize and no-body responses use a nonblocking, nontruncating write-only
descriptor; they do not acquire a new read-permission requirement. Descriptor-open
errors use the prior marking-error policy, without claiming identical errors for
every special-file case. Status200/201/401/403/412 policies remain intact.

FolderUpload retains metadata for its actual opened file and validates the held
descriptor, current pathname and byte count before returning terminal EOF. A
detected changed attempt therefore does not send the final HTTP completion chunk;
the existing retry can reopen a stable version. This is a stronger mutation
consistency contract, not an atomic snapshot or a lock against concurrent writers.

All47 affected package tests pass, including9 uploader and5 folder regressions;
strict all-target Clippy, formatting and fresh example builds pass. The unchanged
original comparisons pass212 filesystem scenarios,10,000 backoff decisions,
32 uploader HTTP scenarios (RS256/ES256 and exact compressed bytes), and99 web
upload cases. Frozen example and selected source/input hashes were rechecked.
The new integration targets run through the existing workspace gate without
feature flags or environment-based skips. Evidence:
`2026-10-19-upload-ownership-audit/FINAL-HANDOFF.json`.

No additional cancellation/retry-cleanup defect was established in the inspected
paths. Exact inspected source identities, unchanged-original controls and native
regression plans are retained privately under
`.analysis/scratch/2026-10-19-upload-ownership-audit/`. Exact committed-SHA CI and
user device/upload acceptance remain separate, outstanding gates.

## Logger shutdown with queued audio (#391)

A real AAC encoder, Matroska muxer and decoder reproduced loss of all buffered
samples when audio arrived before the video header and shutdown followed without
another audio call.1023 samples passed;1024,1025 and3585 samples produced no
decodable audio. The original C++ finish path contains the same missing drain,
so this intentionally fixes inherited behavior rather than asserting unchanged
parity for the defective boundary.

`Audio::finish` now drains complete frames before its existing partial-frame
padding and encoder EOF sequence. Header admission, timestamps, trailer and
incomplete-recording lock policies are unchanged. The four native regressions
now pass, checking decoded sample count after excluding AAC priming and requiring
nonzero signal. All16 loggerd package tests, strict all-target Clippy, formatting
and a fresh daemon build pass. A fresh unchanged C++ source build also passes
ordinary audio, queued-audio and flush comparisons with the candidate, including
media packet/decoded content and raw output/finalization checks.

The first ordinary comparison stopped because the local wheel's FFmpeg lacks
`framemd5`. It passed with isolated Ubuntu6.1.1 tools and their dependencies;
comparison assertions were not modified. All16 recorded original source hashes
match the current #296 checkout. Evidence is
`2026-10-19-logger-audio-391/FINAL-HANDOFF.json`. The permanent regression uses
real host audio media; normal device recording and exact committed-SHA CI remain
separate gates. No recording loss on the user's drive is inferred.

## Stats producer/storage and procfs cache review (2026-10-19)

A bounded source comparison inspected statsd's forked socket ownership,
nonblocking pressure handling, stop boundaries and temporary-file publication,
and proclogd's cadence, PID/start-time cache keys, post-read identity checks,
exit eviction and smaps refresh cycle. No additional concrete defect or measured
optimization was established. The stats logger remains alive for the daemon
loop; the earlier short-lived context-binding defect does not apply there.
Original procfs metadata also discards empty command-line fields.

This was source inspection, not a fresh test run or blanket module clearance.
Sustained-input flush fairness, full statistics numerical behavior, same-name
exec metadata freshness and device performance remain unvalidated by this review.
Exact inspected source hashes and limitations are retained in
`2026-10-19-stats-proc-review/`.

## Sound playback boundaries and diagnostic backpressure (#392/#393)

The original and native playback loops both retained the first callback-local
sample offset/repeat count while crossing a clip boundary. A finite clip with
samples `[1,2,3,4]`, starting at frame2 with five requested samples, incorrectly
returned `[3,4,3,4,3]`. Four native regressions failed before correction; three
controls passed. The native loop now recalculates each contiguous span and stops
at a finite repeat limit. Finite output becomes `[3,4,0,0,0]`; an infinite clip
continues `[3,4,1,2,3]`. Alert selection, volume, scheduling and checked sample
counter behavior are preserved. This intentionally fixes inherited behavior.

Separately, a full diagnostic stderr pipe during a language change held the
playback mutex and stalled the real callback thread in the controlled PortAudio
ABI fixture. The native regression reproduced12 callbacks before/after the
blocked interval. Dropping the playback guard before emitting the diagnostic
keeps callbacks progressing while the same diagnostic remains blocked. The
permanent test requires three additional callbacks and verifies the language
message after draining the pipe; it failed before and passed after the one-line
production change.

All eight soundd tests, strict all-target Clippy, formatting and fresh daemon/
example builds pass. Six policy scenarios cover846 steps and6,348 bit-exact
intended samples. The unchanged original captures remain available:1,113 samples
and27 frame-state rows intentionally differ at the corrected boundary; every
other source state remains exact. The independent reference emits single samples
by linear index rather than copying native spans. All seven controlled runtime
scenarios pass, including live/bounded operation, retries and startup/stop errors.
Selected27 source pins and frozen binaries were rechecked after the proofs.
Evidence: `2026-10-19-soundd-392/FINAL-HANDOFF.json` and
`2026-10-19-sound-lock/permanent-{red,green}.log`.
No physical audio output or device benefit is established; exact-commit CI and
the user's device acceptance remain separate.

## Connectivity request and child ownership (#394/#395)

A deterministic native BlueZ regression reproduced an answered handler clearing
an interactive prompt installed by a newer request. The unchanged Python handler
also exhibits that ordering. Each native answer now retains an internal owner;
completion clears only the matching request. Display updates retain their existing
relationship to an outstanding answer. Respond, Cancel and Release overlap,
expired answers, sender/target checks and actual 60s prompt/90s pairing deadlines
remain covered. This intentionally corrects inherited behavior.

Wi-Fi's tether command previously killed only its direct child and could reap
that leader before cleanup. Two native regressions demonstrated a surviving
owned descendant with both a live and an exited leader. The command now starts
an owned session, observes exit with `waitid(NOWAIT)`, terminates the group while
its leader identity remains reserved, and then reaps. A normal exit 17 control
also passes. The syscall arguments, five-second activation delay, inherited
streams and settings order remain unchanged. Tests use private helper processes;
no host NetworkManager, privileged sysctl or vehicle setting is modified.

Bluetooth's 7 and Wi-Fi's 10 package tests, 17 supervision library tests and its three
CLI targets pass, together with strict all-target Clippy and formatting. All seven
unchanged original protocol/policy comparisons pass, including real deadlines.
The first Wi-Fi original import lacked libzmq; only the remaining two comparisons
were retried with the existing pinned SDK loader path, without changing assertions.
Five fresh executables, the Params binding and 132 source hashes remained sealed.
The existing workspace launcher build now supplies the mandatory test executable;
the UI connectivity job also requires the affected unit tests and Clippy.
Evidence: `2026-10-19-connectivity-owner-review/FINAL-HANDOFF.json`.
Device behavior and measured performance remain separate from these host checks.

## Managed-runtime startup acknowledgement (#396)

Exact-source CI at `e167c37d2d62df62677edbe20c65c1dfb27af9e8` failed one
managed-runtime test while reading the collector's environment immediately before
its shell exec. The test found no prefix in `/proc/<pid>/environ`; an unchanged
local repeat passed 100 times, so the failure is intermittent. The fixture now
acknowledges its actual inherited prefix with its PID, and the test waits for the
complete record. It no longer depends on a procfs environment read across exec.
Real SIGINT, exit 130, five-second cleanup, child reaping, IPC removal and absence
of a success receipt remain required.

Fresh builds of all seven managed daemons and the checker pass 12 integration and
two unit tests, strict Clippy and 100 repetitions of the corrected startup test.
Independent review found no actionable blocker. The 46 CI policy and 13 disk tests
also pass. CI's workspace budget increases from 150 to 210 minutes: the failed run
reached this stage at 114 minutes, while the last successful remaining comparison
and release stages took about 43 minutes. No operation deadline or assertion was
relaxed. The exact new commit still requires remote CI.
Evidence: `2026-10-19-managed-startup-ack/`; remote failure:
https://github.com/bin9208/openpilot-rust/actions/runs/39176145410.

## Wi-Fi snapshot allocation candidate (#397)

`State::snapshot` previously cloned the entire cached snapshot, then discarded
its cloned networks, saved SSIDs and two derived connection fields. It now
constructs all nine returned fields directly. Network sorting is unchanged.
The source patch includes a standalone Rust probe with 46 semantic cases and
paired allocation counts/requested bytes for 8 and 1,024 networks. Its saved
native algorithm is a before/after allocation comparator; the independent
original Wi-Fi policy and protocol comparisons remain mandatory separately.

Under the user's new workflow, this candidate received only local formatting,
syntax and diff checks. Allocation measurements and behavior validation await
GitHub Actions; no numerical saving or device improvement is claimed. The CI
job builds the current Wi-Fi library, requires the probe and retains its output
with the existing connectivity evidence. This adds two growth guards, bringing
the current workflow to 338; the new routing assertions also await CI.

At exact SHA `1364e48c96e888fa5d178c5eb2132d135cf30765`, Actions run39190411681
completed the 46-case snapshot semantic check and the allocation probe. For eight
snapshots of eight networks, allocator calls fell from 256 to 144 and total
requested bytes from 10,856 to 5,760. For eight snapshots of 1,024 networks, calls
fell from 54,672 to 43,720 and requested bytes from 2,574,920 to 1,930,304. Each
difference exactly matches the separately measured discarded clone work. These
are host allocator totals, not live memory, CPU timing or device savings. The job
later failed Bluetooth journal test Clippy before its unchanged Wi-Fi protocol
comparisons, so whole-job and current-source acceptance remain pending.

## Further bounded source review (2026-10-19, cloud execution pending)

Modem external helper admission, interface output and cancellation paths reveal
the lifetime gaps tracked in [#398](https://github.com/bin9208/openpilot-rust/issues/398).
The direct-child cleanup and unbounded blocking waits are inherited; accepting an
exited helper before its descendants close stdout is a native file-capture
difference requiring paired reproduction. Private fixtures and a repair are in
progress. No device reproduction is claimed.

LPA HTTP review covered cookie filtering/order, redirect method/body handling,
response draining and client ownership. No additional concrete HTTP lifetime
defect was found in that bounded review. Header construction has avoidable
intermediate allocations, but no change or measurement is recorded yet.

Lagd source review covered startup cancellation polling, Params writer ownership,
fixed-length point initialization/reset, stable topic timestamp ordering,
estimator acceptance and calibration admission against the retained original.
The inspected reset keeps a prefilled window rather than truncating a partially
filled queue. No new concrete defect was established. Per-evaluation temporary
vectors remain an unmeasured optimization candidate; FFT/numerical internals,
filesystem stall behavior and target operation are not cleared by these reads.

## Resumed cloud failure investigation (2026-10-21)

At `11f3dec4`, Integration passed while Rust and Fast failed. The modem PPP
cleanup regression is documented in #398; the fixed owner retains successful
exit status. Jetlink socket cleanup and blocking diagnostics are documented in
#399 with private baseline/fixed controls. Neither has current fixed execution
evidence yet.

SDK QA #421 exposed a collection-order gap: reporter and barrier use distinct
PUSH sockets, while the raw request collected zero required records. The native
I/O control now requires its crash log before checking the existing backtrace,
log and Params effects. Failed capture observations are persisted before
rejection. The original 40-second collection deadline remains.

UI QA adapter #423 called Rust `BufWriter::write` once, accepting a partial frame
as complete. The adapter now uses `write_all`; the independent generic writer
policy still ignores an injected successful return count, as the original does.
A real short-reading child supplies a new old-source failure control. The earlier
already-exited-child timeout is retained and does not alone establish this cause.
Cloud RED/GREEN and the unchanged UI lifecycle comparisons remain mandatory.

Cweb-push source review covered pinned in-flight request polling, request-client
ownership, redirect methods, address/interface admission and stop propagation.
No additional concrete defect was established in that bounded read. Redirect
encoding creates avoidable temporary strings but remains an unmeasured candidate;
this review does not clear HTTP internals or device behavior.


## Bounded current cloud results and remaining failures (2026-10-21)

At `edb42bf095d6a3e3b38cc0a4546976dfb710363e`, platform job
[118816129959](https://github.com/bin9208/openpilot-rust/actions/runs/39477200877/job/118816129959)
passed the old modem EOF regression, all 15 helper-lifetime checks, strict Clippy
and its remaining original/native platform comparisons. Telemetry job
[118816129992](https://github.com/bin9208/openpilot-rust/actions/runs/39477200877/job/118816129992)
passed the required SDK crash record and full native I/O effects, with the
subsequent tombstone comparisons. These scoped host results supersede the
previous pending status for #398/#421; they do not clear every runtime module.

The aggregate Rust run is not green: the new Panda device gate exposed QA-only
packed-field reference UB (#425) and a native short-help admission difference;
the Audio late-mutation test required unavailable kernel wait-channel visibility
(#426). UI's old-source recording regression did not execute because sharing a
CMake output cache across checkout roots failed before compilation. Sanitizers,
raw byte comparisons, mutation rejection, deadlines and original CLI controls
remain required. Repairs and isolated baseline build output are prepared for
fresh cloud execution. Launcher failed-handshake cleanup (#424) and bridge QA
late-output identity ownership (#427) are separate source-backed findings with
focused regressions in preparation. No device startup, recording, upload or
vehicle acceptance was performed during this pass.


The same workspace run stopped at cluster live-profile's immediate proc-task
assertion (#428), after both repeated owned-close calls succeeded. Source review
follows Input -> MediaInput -> DecodeWorker and confirms the done notification
and owned thread join precede true. The test now waits at most one second for
the exact preexisting task set to reappear, preserving all close, start and
final equality assertions. This addresses the observer boundary supported by
Linux/libc exit ordering, not a measured device decoder leak; fresh CI remains
required. Primary source links and the failing job are recorded in #428.

Installer transport deletion was also traced through its caller: descriptor and
archive destinations are created inside a fresh private `.pending-*` directory
under the installation lock, then extracted, verified and activated. The retry
unlink is therefore not an established deletion of an unrelated caller file in
that actual install path. No speculative transport change or blanket clearance
was made from the earlier name-pattern inventory.


## Lagd masked-input allocation follow-up (#429)

A bounded source review found two temporary masked-input `Vec<f64>` allocations
per `Correlator::masked` call. Borrowed, clonable iterators now feed the same
FFT order and squared-value operations. Existing normalization, rounding,
buffers and daemon policies are preserved. The deep estimation job measures
the same probe against pinned `edb42bf0` and the candidate in separate targets;
it must preserve full-output fingerprints and demonstrate two fewer Rust
allocations per call. No measurement or device CPU result is claimed yet.
See [the focused record](rust-lagd-masked-allocation-429.md); other Lagd and
PocketFFT internals remain outside this bounded review.
