# Complete runtime startup integration boundary

Final composition is tracked in [issue 269](https://github.com/bin9208/openpilot-rust/issues/269),
building on [issue 116](https://github.com/bin9208/openpilot-rust/issues/116),
[issue 120](https://github.com/bin9208/openpilot-rust/issues/120) and the complete
candidate gate in [issue 1](https://github.com/bin9208/openpilot-rust/issues/1).
The manager library from issue 106 is implemented. The issue 269 working tree
now composes its installed entry point; complete-package acceptance remains open.

## 2026-10-12 implementation checkpoint

The native launcher now owns repository-lock handoff, staged-checkout activation,
early recovery, AGNOS update orchestration, external web ownership, model readiness
and manager startup. Six actual launcher invocations with owned filesystem roots
and explicit external providers pass: normal startup, corrupt package, mismatched
source identity, current AGNOS, verified update followed by an owned reboot
recipient, and three updater failures. These are composition checks; the providers
in those six cases do not establish that the full device package is complete.

The installed manager binds executable paths, source working directories, required
companions and original encoder mode arguments. Manager/catalog/bootlog checks
currently pass 19 tests and strict all-target Clippy. Actual manager scenarios with
native bootlog, logmessaged and statsd have reached normal startup and selected
missing-executable handling. Startup-failure screen and shutdown verification are
still in progress. The remaining cluster and GPU model-format implementations,
complete dependency package and exact-SHA CI/device gates remain open.

Fresh child factories now inherit manager fields through `OPENPILOT_LOG_CONTEXT`
while recording their own implementation basename and compiled source identity.
The actual child diagnostic boundary captures typed errors, source chains and
`CarrotException` through the existing reporter. Twenty logging/managed-entry
tests pass, including fresh exec/IPC, malformed inherited context, typed failure,
interrupt and standalone entry behavior. Translated daemon mains now call this
boundary; 46 affected packages pass native-feature compilation and strict Clippy.
The uploader boundary was subsequently added and is checked separately. The collector's
shared formatting library has been separated into `openpilot-log-format` with
all eight implementation files byte-identical to their previous versions and
the collector's public exports retained. Fresh-cache formatter/collector/logging
and managed-entry tests pass.

Seven actual executables (`controlsd`, `radard`, `plannerd`, `paramsd`, native
`proclogd`, `uploader`, and `bluetoothd`) now pass an owned failure-to-collector check. The uploader
first failed because its entry emitted only stderr, then passed after the same
managed entry boundary was applied. Each emits exactly one
crash record with its typed cause, backtrace, inherited device/version fields and
actual implementation basename; the normal reporter writes `CarrotException`.
Their standalone help paths also pass. `rust/tools/check_managed_runtime.py`
records each executable hash and collector hash in its result.
The Bluetooth boundary now consistently uses catalog identity `carrot_bluetooth`.
The original inherited global context already preserved that log identity, but
without inherited context the local binding incorrectly used `bluetoothd`; this
was reproduced and corrected. Both actual collector modes now pass, and the SDK
tag receives the same corrected literal.

The separate `check_runtime_log_upload.py` now exercises that failure through
the actual native collector, continuous route logger and uploader. One typed
crash was decoded from the uploaded qlog, all 1,648 transmitted bytes matched
the logger's file, and the uploader set the existing completion xattr. Source
identity and daemon/implementation fields survived the route log. Storage,
signing keys and the HTTP recipient were owned host fixtures. This closes that
logging/upload connection; it does not establish the complete managed catalog
or physical startup. The check is wired into the logger CI job.

`rust/tools/seal_runtime_package.py` checks a staged `rust/bin` against the native
manager inventory before writing the launcher's hash manifest. Missing implementations,
empty companions, scripts in executable slots, wrong ELF architecture and links
outside the checkout are rejected. Only the source-absent optional webjoystick is
exempt. Five focused package tests pass; this does not replace recursive provider ABI,
model content or normal-startup checks. Its CLI requires a clean source checkout.
Startup verification also rejects installed `rust/bin` files omitted from the
manifest; the omission test failed before the check and now passes with the four
existing manifest tests.

Cross-worktree reuse of one Cargo target directory produced a stale UI build
script and replaced a corrected text-window basename with another worktree's
binary. Final issue 269 checks therefore use a fresh, dedicated `runtime-269`
target directory. Other active worktrees use separate namespaces. Final owned
scenarios stage independent copies and record their hashes before execution.

Current local receipts are `bootstrap-first/result.json` and
`managed-daemon-final/result.json` and `uploader-entry-green/result.json` under the runtime-package scratch directory,
`native-route-upload-first/result.json`, plus the manager evidence directory
for issue 269. They identify
the observed binary hashes; this checkpoint is uncommitted implementation evidence,
not an exact-SHA CI result, complete-runtime handoff or device measurement.

## Existing components and remaining connections

`rust/crates/manager` provides initialization, lifecycle, ordered process
ownership, native startup/exit adapters and the continuous IPC loop. Its original
native example deliberately binds a synthetic child. The installed executable must
bind each selected catalog entry to its actual packaged Rust executable,
arguments, working directory and persistent-process policy. Source Python module
names are provenance, never fallback launch targets. Keep a selected missing
implementation as an error and account for source-disabled entries explicitly.

The catalog's package/binary pair is insufficient as a launch binding. Current
entry points have these concrete composition requirements:

| Entry | Required binding |
|---|---|
| `card` | `--root` and `--numerics` are mandatory; bind the packaged checkout/DBC assets and verified numerical provider |
| `xiaoge_data` | `--root` is mandatory and canonicalized; default assets and config resolve beneath `openpilot/selfdrive/carrot/xiaoge` in that root |
| `modeld`, `dmonitoringmodeld` | `--trusted-catalog` is mandatory; supply the correct immutable model catalog for each daemon |
| Encoder variants | Preserve each source mode argument such as `--stream`, `--carrot-vision-road` and the four YouTube choices while selecting the single native encoder executable |
| `controlsd`, `torqued` | Resolve their numerical companion directories; controlsd also reads torque assets relative to its working directory unless given `--assets` |
| `plannerd` | Bind `--solver` or `PLANNER_ACADOS`, otherwise the pinned solver is expected beside the executable as `plannerd-acados` |
| `radarcan` | Bind `--numerics`/`RADARCAN_NUMERICS` and `--dbc`/`RADARCAN_DBC`, otherwise both directories are expected beside the executable as `radarcan-numerics` and `radarcan-dbc` |
| Native subprocess users | Keep `openpilot-process-child` beside the executable, or pass the supported explicit launcher path |

This table originally came from reading the native argument parsers. Installed
binding checks are now part of issue 269. Resolve normal arguments independently of
fixture-only frame limits, fake providers and phase fences.

Managed diagnostic composition also remains open. The source Python launcher
adds the daemon log/Sentry tag and catches entry errors inside the child;
the source native launcher instead sets `MANAGER_DAEMON` and execs. The native
`managed-entry` library implements the former boundary. Before issue 269,
workspace manifests linked it only from manager and Athena. A generic
parent wait cannot recover a typed error from another executable. Each
translated Python entry must therefore compose its actual child diagnostic
boundary, while retaining the source distinction for originally native entries.
Manager logging binds a process-local factory context; before issue 269 a fresh
daemon `Factory::for_runtime()` started with an empty context. Exec needs the
explicit context export described above to preserve source fork fields. Verify the installed
child's context and error event through the native collector and existing
upload path, rather than only checking manager logs.

The source outer `manager.py` entry point owns `helpers.unblock_stdout`, startup
failure reporting, stopping the UI before the error window, and final status.
The installed native entry now creates a controlling PTY through the process-child
helper, forwards SIGINT/SIGTERM, drains 4096-byte packets to nonblocking stdout,
drops undecodable/unwritable packets, and reaps the child with source exit status.
The helper executes session/terminal setup before exec; it does not fork threaded
Rust. The web-terminal session API retains its separate no-controlling-terminal
contract. Launcher-lock ownership passes through the stdout child unchanged. Existing native
spinner/text-window executables supply the display boundary; a library error
alone does not replace the original startup failure screen.

`launch_chffrplus.sh` currently orchestrates repository locking, staged-checkout
activation, early recovery service, AGNOS checks/updater UI, dependency/build
readiness, the external web watchdog, model readiness/background updates and
manager startup. Map these project-owned decisions to native owners and staged
artifacts. In particular:

- Recovery and external web children must close the inherited repository lock.
  Keep duplicate-worker prevention and `CARROT_WEB_EXTERNAL` ownership.
- Required AGNOS verification, automatic update/retry/reboot behavior and Wi-Fi
  access remain unchanged. Use the native update/UI implementations; do not keep
  Python dependency installation as a hidden runtime prerequisite.
- The final package must provide the actual external libraries and model
  artifacts required by the native executables. Generic host/aarch64 binaries
  alone are not an AGNOS installation package.
- The native GPU probe, compiled eGPU artifacts, model provisioning and
  background delivery depend on unfinished issue 154. Existing launcher calls
  into `modeld.helpers` and `big_model` cannot remain as the final implementation.
- Preserve the source AGNOS manager bootstrap affinity on cores 0 through 5,
  each daemon's explicit overrides, the boot snapshot and diagnostic log paths.

## Independent bootstrap services still unported

[Issue 263](https://github.com/bin9208/openpilot-rust/issues/263) tracks the
standalone recovery server at `openpilot/selfdrive/carrot/recovery/server.py`.
The launcher starts it on port 6999 before AGNOS verification and build
readiness. The native startup UI already displays this address; that display
is not evidence of a native recovery listener. The recovery entry point is not
registered in `process_config.py`, so the registered-process inventory alone
does not cover it.

Recovery owns a separate persistent login PTY, HTTP/WebSocket routes, Git/tool
operations, tmux upload and CwebPush recovery control, plus its own support
session/PIN/approval/expiry/tunnel lifecycle. It reuses the main terminal's
frontend assets but must remain usable when the main web executable or its
codec/native providers are unavailable. Reuse neutral Rust process/PTY and
protocol foundations, with a distinct runtime owner and a minimal dependency
closure. Its reachable project Python command invocations also require native
replacement; standard-library-only Python is still a runtime dependency.

Recovery is not an alias for the main server: it uses a threaded HTTP/1.1
listener, a 64 KiB JSON body boundary, source-specific query parsing and HEAD
handling, and a second listener for the support guest. Its inline HTML and two
JavaScript responses are existing frontend assets to preserve. Reuse already
converted Git/update and upload primitives only after comparing this standalone
wrapper's actual requests, error/status fields and timeout rules. Do not pull
main-server codec, model or camera dependencies into the recovery executable.
The current `carrot-server` crate has an unconditional FFmpeg dependency and a
large service closure. Recovery must not depend on that whole crate merely to
reuse terminal or Git code. Shared process primitives already live below it;
extract only a proven common policy boundary where both consumers need it, or
retain the standalone source wrapper when its behavior differs. Verify the
recovery ELF's linked libraries and startup with main-server providers absent.
Its threaded PTY reader also accesses shared process/fd fields across teardown;
exercise reset and old-reader completion independently. The main server's
reproduced reset defect in issue 264 is a related ownership warning, not evidence
that the recovery variant has already been reproduced or corrected.

`scripts/carrot_web_watchdog.sh` is a separate launcher integration boundary.
Preserve duplicate detection through the process table, PID file and flock;
PID-file cleanup must check ownership. Its loop re-enters the physical checkout
before every server restart, so an updater replacing the checkout cannot leave
it running from the removed inode. It owns external Carrot Web on port 7000
through `CARROT_WEB_EXTERNAL`, with the original retry delay and signal policy.
Both watchdog and recovery children must release the boot repository lock.

Recovery implementation/owned HTTP-PTY evidence and native watchdog composition
remain pending. Their absence prevents a complete-runtime handoff even when
all registered daemon candidates have passed isolated checks.

The Carrot server's native QR dependency repair requires a Brotli provider
bundle at `<repository>/rust/bin/brotli`. The bundle contains the encoder,
decoder and common libraries plus a manifest identifying the Linux target,
provider version, fixed filenames and SHA-256 hashes. The runtime validates and
stages a complete generation under `<CARROT_DATA_DIR>/native-deps/brotli`, then
atomically selects it. This replaces Python wheel/pip installation with repair
from the verified native package. Final packaging must supply the actual
same-target provider closure and its license/provenance; host fixture copies
do not satisfy this target requirement. System-provider fallback and the CQR4
fallback remain explicit when the native encoder cannot be used.

The package's codec closure also covers the Carrot server's native YouTube
FLV/AAC writer and WebRTC debug video, in addition to encoderd. Bindings and
libraries must be built against the same target FFmpeg ABI and available
encoders; successful host comparisons do not establish that target closure.
YouTube retains external `librtmp.so.1` behind an opaque owned handle and uses
native TLS with the target OS trust roots. Include both the library dependency
closure and certificate-store path in packaging checks. Keep native-provider
diagnostics truthful rather than reporting Python/PyAV provider names.

The WebRTC host ELF dependency audit at `ea3302e8b` records direct
`libavcodec.so.60`/`libavutil.so.58` imports and host symbol requirements through
GLIBC 2.38 and GLIBCXX 3.4.30, without a runtime search path. Its Ubuntu codec
provider resolves a large transitive closure; copying that host installation is
not target packaging. Build the selected codec set against the AGNOS sysroot,
including libvpx/libx264 and its license closure, then verify every packaged
executable and library. `libavformat` is a current binding/build dependency but
is absent from this particular ELF's direct imports. The same ELF contains
external ZeroMQ symbols from the existing Rust `zmq` dependency; C++ runtime
imports alone do not establish retained project-owned msgq implementation.

CarrotMan's normal GEOS-enabled path separately requires the pinned library and
manifest described in `rust/crates/carrot-man/native-dependencies.json`.
Packaging must not silently select dependency-unavailable behavior to avoid
shipping a required provider. The final executable/dependency audit must also
reconcile historical CXX entries in `rust/port-status.json` against the actual
linked code. The USB ownership adapter was converted to Rust in the issue 154
checkpoint `da9b6a3e50b487a6d0aea9ad96580e0ddc843dca`; external libusb remains a
provider. The later 473+473-kernel host replay matches prediction and next-state
bytes using a shared loader/emulator and zeroed recurrent inputs; independent
loader, long recurrent sequence and physical GPU acceptance are not established.
The issue 154 native provisioning, generic `run` HCQ adapter and local
`run_policy` adapters are now integrated into issue 269, including AMD preparation
and QCOM preparation followed by native AMD policy execution. Readiness uses the
verified `usbgpu-assets` directory beside the selected executable; installed
manager exports its selected binary directory's companion root unless
`USBGPU_ASSETS_ROOT` is explicitly configured. Modeld, the model CLI, UI and Carrot
server honor that selection. A downloaded Python entry point or ONNX
chunkmanifest alone does not establish native readiness. Carrot server reports
the active native package independently from a previous-model fallback. The
final target package must supply the same complete companions to every caller.
The historical catalog `comma-run-model` format remains explicitly unavailable:
the matching `model_runtime.py` artifact and queue contract have not been found
in the retained runtime archive or the recorded primary-source tree inventories.
No adapter contract was guessed. The cluster
encoder/decoder bridges remain project-owned conversion work.


The approved design permits Python model conversion during an explicit build
phase. The source ONNX path runs through `manager/build.py::build_usbgpu_model`
and SCons before manager startup; this does not require replacing the general
ONNX/Tinygrad compiler. The installed execution path must instead consume
verified native companions. The generic catalog build helper and local captured-model exporter publish
immutable native exports before startup. Runtime generic `run` and local
`run_policy` paths consume their verified queue/buffer and warp companions;
downloaded Python `make_input_queues`/`MODELD_INPUTS` and runtime warp compilation
are not execution providers. The remaining historical catalog `run_model`
boundary requires its matching source artifact before a native contract can be
implemented.

The dependency audit must distinguish runtime linkage from comparison tooling.
For example, the current `msgq/build.rs` compiles standalone original C++
`native-msgq-peer`, `native-vision-peer` and `native-ipc-abi-peer` executables
with Cargo link metadata disabled. They are source-oracle tools, not proof that
the Rust daemon links the original C++ transport. The stale per-process CXX
transport labels need reconciliation against the selected package and its
actual runtime dependencies. Small wrappers for external solvers, graphics and
codecs remain allowed by the approved design; record their provider, ownership
contract and linked implementation instead of classifying all `.cc` files as
unported daemons. Conversely, project-owned service/protocol decisions must not
be hidden behind that external-provider category.

## Installed manager composition (#269)

`openpilot-manager --root REPOSITORY` now composes the native startup,
registration, boot snapshot, repository-lock release, catalog, cereal manager
loop and exit boundary. `--bin-dir DIRECTORY` selects an explicit installation;
the default is `REPOSITORY/rust/bin`. `--inspect` prints every source catalog
entry with its bound argv, working directory, required artifacts and readiness
error. Source Params/Paths environment, `BLOCK`, `NOBOARD`, `PREPAREONLY`,
persistent daemon PID keys, enabled predicates and stop/restart policies remain
the deployment controls. There is no Python fallback or placeholder daemon.

The manager receives an owned CLOEXEC lock file over `CARROT_BOOT_LOCK_SOCKET`.
Initialization checks its device/inode against `CARROT_REPO_LOCK_PATH` and
releases it through the existing finally boundary. The entry sets `BASEDIR`,
`OPENPILOT_BASEDIR` and `OPENPILOT_ROOT` from its selected root. Registration honors
the source `API_HOST` override, defaulting to `https://api.commadotai.com`.
Raw unused host strings retain source behavior;
the HTTP adapter parses them when a registration request uses them. Non-UTF8
configuration fails explicitly instead of silently selecting a default. The entry exports the
initialized source log context before process preparation so native children
retain version/origin/branch/commit/device and use their own implementation and
daemon names. Startup errors go to the original rotating file formatter and
native text-window child. Ordinary child stdout/stderr inherit the manager PTY;
the outer forwarding parent absorbs saturated or closed output recipients.

Package bindings require native ELF executables with execution permission, the
native process-child helper and source working directories. Missing selected
entries fail explicitly. Directory checks establish path presence; each
provider's own loader and the package manifest still own content/hash/ABI
verification. Companion paths are:

| Catalog entry | Companion or source asset binding |
| --- | --- |
| card | `rust/bin/card-numerics`, selected repository root |
| controlsd | `rust/bin/controlsd-numerics`, `opendbc/car/torque_data` |
| torqued | `rust/bin/torqued-numerics` |
| radarcan | `rust/bin/radarcan-numerics`, `rust/bin/radarcan-dbc` |
| plannerd | `rust/bin/plannerd-acados` directory |
| modeld | `rust/bin/driving-models` trusted catalog with `current.json` and verified generation |
| dmonitoringmodeld | `rust/bin/driver-models` trusted catalog with `current.json` and verified generation |
| pandad | native daemon beside supervisor, `panda/board/obj` firmware, selected repository root |
| soundd | `openpilot/selfdrive/assets` |
| ui | selected repository root, including its original UI assets |
| xiaoge_data | selected repository root |

Encoder variants preserve their source stream/profile arguments; jetlinkd adds
its actual continuous `--run` entry argument. `manage_athenad` retains the
`AthenadPid` persistent process policy. `carrot_cluster` remains unported in this
catalog until a full scene/runtime entry exists; QA examples are not admitted.
`webjoystick` is source-disabled when the optional bodyteleop module is absent.

The focused host scenario is `installed_manager OUTPUT REPOSITORY BINARIES MODE`
(Cargo example), with modes `normal`, `missing-daemon`, `missing-bootlog` and
`missing-bootlog-ui`. It drives actual manager startup with genuine bootlog,
logmessaged and statsd executables, owned Params/msgq/storage and a real flock
transfer. The source `BLOCK` control limits this connection test to those
children; this is not a complete runtime candidate. Synthetic invalid device
packets exercise road-state changes without writing the host power watchdog.
The text-window dismissal status defect discovered here is tracked separately
in [#272](https://github.com/bin9208/openpilot-rust/issues/272): its native binary
must return source status 1 after the Exit button, which the manager wrapper
waits for. Local exact-byte command receipts and captures are under
`.omo/evidence/269-manager/` in the #269 worktree.

The independent review fixes are recorded under
`.omo/evidence/269-runtime-review-fixes/`. The saturated-pipe reproduction blocks
the former installed manager before boot-lock connection. The native output
boundary and original source function each complete one-MiB writes against full
and closed pipes with both INT and TERM, preserving blocking child stdio, the
controlling terminal, three-second cleanup and exit 7. Actual installed-manager
pressure cases retain native IPC/bootlog/logging and child cleanup. A stopped
genuine statsd child makes INT, TERM and nested handoff wait 6.07–6.08 seconds;
all selected native children are gone afterward. Manager/overlay handoff forwards
the original signal and waits, while the external web server retains its bounded
shutdown policy. Bootstrap paths/environment and background services/model
updates now have separate concrete modules. The four manager controls and six
owned bootstrap controls were replayed with new immutable provider copies.
Independent re-review and the complete package gate remain open.

## Evidence required at this boundary

Use owned roots and recipients to observe the actual launcher/manager entry
points: normal startup, required-artifact failure, startup-error display,
repository-lock transfer/release, external service ownership, stdout pressure,
signal forwarding and child cleanup. Reuse completed component comparisons;
add only the missing connections and failures at this boundary.

The composed candidate must identify its implementation and commit in the
existing logs, record through the native logger path and exercise the existing
upload flow against an owned recipient. Inspect the packaged dependency and
executable closure for project-owned Python or C++ runtime paths that have not
been converted, and leave any remaining item explicit in the inventory.

This file records source inspection and scoped owned host startup evidence.

The native launcher now prepends `<repository>/rust/bin/lib` to its children's
`LD_LIBRARY_PATH`, retaining explicit inherited search directories and avoiding
an implicit current-directory entry when the original value is empty. The
extended actual-bootstrap provider recorded an empty library path before the
fix (RED); all six startup/error/updater scenarios now receive the package path
(GREEN), including unset, explicit and empty inherited values. Strict launcher
Clippy and the Python harness checks pass. The invoked launcher SHA256 is
`965d812b82a01682e20690edc09aaa928912a3a8ca1b08e6b3753997abda04b0`.
Evidence is `.omo/evidence/269-native-library-{red,green,clippy}.log` with the
owned child records under the corresponding `native-library-*` scratch runs.
This proves startup path propagation; the actual target library and ABI closure
remains a separate package gate.

Complete runtime composition and device acceptance remain pending. The user performs the first C3X comparison only
after the complete candidate passes its host/target build and composition gates.


Issue 269 GPU integration checkpoint (2026-10-12):
`.omo/evidence/269-gpu-integration/receipt.json` identifies the exact source files,
commands, frozen host providers and owned inputs. Fresh native structural
admission passed the genuine generic checkpoint at both camera resolutions and
both retained small local AMD/QCOM package variants. Six production driving-loop
IPC scenarios (disconnect, retry and cancellation at both resolutions) reaped
all owned workers and shared files. Four installed-manager controls observed
normal/explicit companion roots in actual native children, bootlog/lock ordering,
IPC transitions, collector context and explicit missing-artifact failures.
Four actual HTTP readiness controls cover the valid package, missing companions,
Python-only marker and a verified previous package while the active package is
unavailable. These are host process/IPC and structural evidence; trained-model
physical GPU correctness, AGNOS provider closure and the complete normal
startup/upload candidate are separate gates. The prior local recurrent and
473+473 kernel comparisons were not rerun for this integration.

## Target media providers (2026-10-13)

`rust/tools/build_runtime_ffmpeg.py` builds the existing pinned FFmpeg 6.1.1
archive (`8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968`)
with bounded `-j2`, disabled autodetection, shared libraries, CLI/ffprobe,
libx264/libvpx/OpenSSL/zlib, and GPLv3 license/source records. Each build phase
checks the 25 GiB reserve plus 3 GiB growth. The first build failed only at strip
because the extracted host binutils needed its own shared-library directory;
the explicit `--binutils-libraries` option resolves that host tool dependency.
Reconfiguration, linking and installation then passed.

The SDK combines providers extracted from the pinned AGNOS image with arm64
libvpx 1.14.0-1ubuntu2.3 obtained from Ubuntu's signed package index. SDK and
package hash records remain with the current provider build. FFmpeg retains
the project's pinned source instead of using a newer system FFmpeg ABI.

`rust/tools/validate_runtime_ffmpeg.py` passed the dependency/version/strong-symbol
closure of 18 ELF objects and ten actual AArch64 QEMU commands using the pinned
AGNOS loader and explicit AGNOS/package library paths. The commands encode four
96x64 frames through libx264, VP8 and VP9, inspect each container with ffprobe,
and decode each back to four complete RGBA frames. The provider identities are
FFmpeg `f2cc8824d08d9d4d4e6beb36e89d10ef69bb0a8c2ea7f1840f23ca456b1847c8`
and ffprobe `bb8992db3348212b870083099f60c2771dbe2d490b76205c824899637beb4e46`.
The report is `.omo/evidence/runtime-269-ffmpeg/report.json` in the main checkout.
This is loader, ABI and software media execution evidence. It does not establish
physical GPU behavior, vehicle performance, the full package closure, or the
normal startup/upload delivery gate.

The SDL provider check uses only the seven native libraries from the pinned
Pygame 2.6.1 aarch64 wheel, retaining SDL 2.28.4, SDL_ttf 2.20.1, FreeType 2.12.1,
HarfBuzz 5.1.0, libpng 1.6.37 and Brotli 1.0.9. No Python runtime files are selected.
`validate_runtime_sdl.py` verifies the wheel/source hashes and 14-object ABI
closure, creates an owned dummy window under QEMU with the AGNOS loader, then
renders text. All 9,880 RGBA bytes equal the original host Pygame renderer;
the pixel SHA256 is
`9a6350373163bb1f00b8051679bc045297e8b298d6ef90d539c343b04622128e`.
The retained report is
`.analysis/scratch/2026-10-13-runtime-providers/sdl-validated/report.json` in
the main checkout. Component license texts and the bundled FreeSans font's
original GNU GPL notice remain separate; the font is not labeled LGPL.

`stage_startup_ui_raylib.py` now accepts a verified cached wheel and explicit
target library directory, checking disk reserve before extraction and linking.
Rebuilding against the pinned AGNOS graphics libraries reproduces the prior
plugin hash `f62d902f04c9b329d1a8f0741e9ca83dd7c3b92beddaea4a3ec393b1f4bafa44`.
The current-source check now covers 117 raylib exports, five EGL exports,
25 GLES exports and three extension pointers, with all four actual QEMU loader
invocations and the 28-object ABI closure passing. The checks read the current
native adapters' symbol lists; they do not initialize a display or GPU.
`.omo/evidence/runtime-269-graphics/report.json` records exact sources, commands,
file hashes and results. Physical display, DMA import and device performance
remain user-validation boundaries after complete runtime packaging.

The staged media providers also pass their checks from `rust/bin`: the initial
48-object, 19-root ABI audit, ten FFmpeg commands, SDL text pixels and the current
raylib symbol contract. Their reports are under
`.omo/evidence/runtime-media-staged/`. The SDL check requires the owned dummy
video driver; an earlier invocation without that setting failed under QEMU and
is retained in the report. No physical display was used.

The first bounded target workspace build produced 82 AArch64 executables, with
Xiaoge and webcam held for their OpenCV provider and the cluster awaiting its
final integration. `.omo/evidence/runtime-target-build/` records the build
environment, Cargo artifact identities and staged hashes. The actual entry-point
ABI audit passed 146 ELF objects across 115 roots. BLASFEO and HPIPM are checked
inside the two solver load closures, rather than as independent load roots;
their math symbols are supplied by those closures. Twelve actual AArch64 planner
solves using the staged acados libraries equal the original saved ARM results
bit for bit (`runtime-media-staged/plannerd/report.json`).

Two installed provider paths were corrected after failing focused tests.
Carrot Server's QR repair now uses the sealed `rust/bin/brotli` bundle, containing
the pinned AGNOS Brotli 1.1.0 libraries, manifest and license record. Six actual
Rust AArch64/QEMU steps verify installation, reuse, CQR4-to-CQR3 fallback and a
settings round trip without network access or a system-library fallback.
Carrot Man now discovers the installed pinned GEOS bundle when no explicit
library override is supplied and selects its AArch64 manifest on that target.
Four original saved route-helper controls exactly match the target provider
without a manifest override. The manager catalog requires both companion
directories. Focused tests and strict library Clippy pass; reports are under
`.omo/evidence/runtime-package-providers/`.

The additional system-provider audit reads the current Rust dynamic symbol
contracts and verifies the pinned AGNOS libportaudio, libusb and librtmp closures:
23 ELF objects and 10, 18 and 12 required symbols respectively. QEMU resolves the
symbols without opening hardware or network connections. This report is
`runtime-media-staged/system-dynamic/report.json`.

These are component and target-provider results. The final integrated source
must still be rebuilt and sealed, including the internal GPU model catalog,
before the normal startup and existing log-upload candidate is complete.

The current actual host manager inspection finds 59 ready entries among 63:
the missing driving/driver model companion directories, unfinished cluster
binding, and absent disabled source webjoystick remain explicit. This checks
installation requirements, not successful execution of every daemon. Inspection
also exposed a sealer mismatch: Rust serializes `OsString` arguments as Unix byte
arrays, while the sealer expected strings. The sealer now decodes that native
format without lossy Unicode conversion and rejects malformed encodings. Six
new cases failed before the fix; all 12 package tests now pass, Ruff passes, and
every argument from the actual manager output decodes exactly. The current
inventory and provider hash are in
`.omo/evidence/runtime-package-providers/inventory-report.json`; this is not a
sealed candidate. Manager-catalog strict Clippy also passes.

`check_manager_log_upload.py` now joins the actual installed manager example to
the existing native uploader. The manager starts bootlog, logmessaged, statsd
and loggerd through the native process launcher, performs offroad/onroad/offroad
transitions, records a deliberately malformed-byte stats input as a managed
failure, and restarts that child. The resulting rlog contains 466 messages and
41 child log records; the qlog contains the typed crash with Rust implementation
identity. The existing uploader sends the unchanged 8,179-byte qlog and bootlog
to an owned loopback recipient and marks the qlog uploaded. All four observed
child PIDs, including the restarted child, are reaped.

The final report is
`.omo/evidence/runtime-manager-log-upload/composed-fourth/result.json`, with
exact hashes for seven native executables and the probe. Initial harness runs
incorrectly expected ordinary logMessage records in qlog, then lacked the
uploader's newly owned IPC namespace, then expected automatic rlog upload;
the final harness follows the original service and upload policy. Production
logging/upload behavior was not changed. Manager example strict Clippy and the
Python harness Ruff checks pass. Other daemons are blocked in this owned host
composition; it does not replace the complete package or device acceptance.

The external asset audit found Athena's offroad snapshot still launching the
legacy `openpilot/system/camerad/camerad` path. It now starts the native
`openpilot-camerad` beside its configured process launcher. The existing-process
gate recognizes the native executable's 15-byte Linux process name as well as
the original camerad name. With only the native installed path present, the
old implementation failed to start its camera child; the updated implementation
passes actual process launch, frame-80 VisionIPC gating, original JPEG equality,
child reaping and Params cleanup. Separate real existing native and legacy
camera-name processes are both left running. Strict Athena Clippy, formatting
and the updated harness Ruff check pass. Exact provider/source hashes and
RED/GREEN results are under `.omo/evidence/269-athena-native-camera/`.
The camera peer is an owned original-protocol fixture, not physical hardware.

Required data files now join the sealed manifest even when their paths are
outside `rust/bin`. Missing, empty and unhydrated Git LFS files are rejected;
the four new admission cases failed before the change and all 16 package tests
pass afterward. The existing launcher already hashes these repository-relative
manifest paths on startup. The asset audit also led to hydration of the two
web warning SVGs from their exact LFS objects: `alert_camera.svg` is 989 bytes
with SHA256 `f1086279a32f49f68de1db4e89a93aa2aae779395cfb908e5f6a0cbf2bf7dae6`,
and `alert_police.svg` is 707 bytes with SHA256
`d0dea6745d5cd5cf8206ced256310fed587555b3309d80cad46a4c066a931401`.
Both remain Git-clean. The raw driving ONNX was also hydrated from its already
cached exact LFS object (`f73a9e535523d5e9acb9e642c64e33d631825dc8ba74123757d107cedd047bb5`).
The final device candidate will use the repository's optimized release profile;
the earlier debug-profile AArch64 executable staging is intermediate evidence.

The normal `launch_openpilot.sh` entry now resolves its own checkout and execs
`rust/bin/openpilot-launch`, forwarding its arguments. It no longer dispatches
the legacy Python startup script. Before this change, an owned package containing
only native launch providers failed at the absent legacy script. All six
bootstrap scenarios now pass from the actual shell entry, including normal
startup, package corruption, metadata mismatch and the existing AGNOS paths.
The report `.omo/evidence/269-normal-entry/green/result.json` binds the entry,
native launcher and explicit external-provider hashes. Bash syntax and harness
Ruff checks pass. This branch requires the complete sealed native package to be
installed beside the source before device use; the user performs device
installation and the first comparison after the complete candidate is ready.

The cluster implementation is now imported into the integrated workspace:
673 files were checked against source and destination hashes before copying,
and shared changes were applied separately to preserve the existing runtime
integration. The first AArch64 build identified the previously committed USB
descriptor/scanner additions missing from the import; those six files were
included and the native-media build then passed. Both staged cluster entrypoints
run their help path under the AGNOS loader and their 15-object ELF dependency
closure resolves. These are unoptimized integration artifacts; the final release
build and complete package admission remain separate.

The manager catalog now resolves `carrot_cluster` to the native autorun entry,
passes the checkout root and requires its native helper, FFmpeg/FFprobe, replay
font, default Korean font and eight display images. The new binding test failed
before registration; all seven binding tests and strict catalog Clippy pass.
Evidence is in `.omo/evidence/269-cluster-import/`. Cluster visual and lifecycle
controls remain documented in `rust-cluster-253.md`; target help/ABI checks do
not establish physical USB/display behavior.

Package sealing also requires a nonempty native `usbgpu-assets` companion even
though the bootstrap model selector is outside the ordinary manager inventory.
The new missing-companion case failed before that check; all 17 package tests
pass afterward. This directory check alone does not establish valid model
selection or inference: the exact genuine source artifacts and their consumer
contracts must also be admitted before sealing the complete candidate.

The two GPU helpers used indirectly by the model selector and diagnostic path,
`openpilot-usbgpu-worker` and `openpilot-usbgpu-probe`, are now required native
executables in that same package gate. Previously both missing-helper cases
could be sealed; they now fail admission, and all 19 package tests pass. Ruff
also passes. These tests are included in the required workspace CI job;
local RED/GREEN records are under `.omo/evidence/269-usbgpu-assets/`.

## Committed composition and first PR validation (2026-10-13)

[PR #273](https://github.com/bin9208/openpilot-rust/pull/273) starts from
`4fc2381497dea2f80cddffad50e72691b32975f4`. The clean committed host build passes
the six normal shell/bootstrap cases and the composed manager/bootlog/collector/
statsd/logger/uploader run against owned IPC, storage and loopback HTTP. The
recorded native log identifies that SHA and `source_tree=clean`; the failed child
restarts, all four native children are reaped, and boot/qlog upload preserves the
generated bytes. These remain host boundary checks with other daemons blocked.

The initial [Rust CI run](https://github.com/bin9208/openpilot-rust/actions/runs/38309467609)
exposed a working-directory mismatch in the upload checker: CI invokes it from
`rust/`, while the uploader reads the version header relative to the normal
repository working directory. The same command reproduced an empty HTTP capture
locally from `rust/`; setting the checker child working directory to the source
root made the real upload pass without changing uploader policy. The composed
manager checker now also sets its uploader working directory explicitly and
passes from `rust/`.

The UI application CI job also omitted the native process-child executable
required by its updater-worker tests. It now builds that helper before the tests.
The captured missing-helper failure is reproduced with an unavailable helper;
the existing 13 application unit tests pass with the real helper. Neither fix
replaces exact-SHA CI, final optimized package validation or user device acceptance.

Two jobs shared a stale startup-window expectation: normal dismissal intentionally
returns 1, while their checker still expected 0. The checker now requires 1,
first waits for the rendered button, and verifies that an outside click leaves
the process alive. All six original/native scenes retain exact English/Korean
text pixels and source state; actual click, spinner input/termination, child
lifecycle and the instrumented adapter checks pass. External raylib remains
uninstrumented. The production UI was not changed for this correction.

The isolated encoder CI enables FFmpeg software scaling. Its host job now
installs the missing `libswscale-dev`. The pinned ARM FFmpeg 7.1 wheel omits
swscale, so its staging tool supplements only that library and headers from the
official 7.1 source archive, SHA256
`40973d44970dbc83ef302b0609f2e74982be2d85916dd2ee7472d30678a7abe6`.
The wheel's existing libraries remain byte-identical; the scaler source must
match its avutil version/pixel-format headers. The stage records commands,
artifact hashes and LGPL notices. Twenty encoder fixture tests pass, including
source-checksum and ABI-header rejection. An actual AArch64 RGB-to-YUV consumer
linked to the augmented provider runs in QEMU with zero mismatched pixels. This
generic CI provider is separate from the final AGNOS FFmpeg 6.1.1 package.

The same initial run reached the Xiaoge native-memory artifact selector after
successful test/example builds, but `--examples` also built the newly added
cluster `jpeg_resources` program. That selector deliberately admits exactly its
two oracle programs. Its build command now explicitly selects `opencv_trace`
and `jpeg_options`; the strict selector, complete native test build and sanitizer
flags remain in place. All five existing Xiaoge CI contract tests and Ruff pass;
the complete sanitizer execution still requires the next exact-SHA cloud run.

The workspace run also reproduced the recovery tunnel timeout failure in 27.03
seconds. Normal recovery initializes child-subreaper ownership; its isolated
support test driver had omitted that initialization. Descendant cleanup then
reported an ownership timeout, and the tunnel's early error return skipped
waiting for its directly owned leader. The test now reproduces normal startup,
while a separate uninitialized-driver regression preserves that failure path.
The production cleanup always waits for the leader before returning the original
descendant-cleanup error. Captured parent/session IDs demonstrated the old zombie
leader and its absence after the fix; only specifically owned fixture children
are reaped by the test. All 19 recovery library tests pass, with three unchanged
isolated child-driver entries, and the 25-second timeout and strict `/proc` checks
remain unchanged. Strict all-target Clippy and the focused independent review
pass. Evidence is in `.omo/evidence/recovery-tunnel-ci-20261013/` and
`.omo/evidence/273-recovery-cleanup-review.md`.
