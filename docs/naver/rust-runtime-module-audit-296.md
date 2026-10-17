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

No blanket module clearance, complete Python retirement, device boot acceptance,
loaded-driving acceptance or measured CPU improvement follows from this matrix.

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
