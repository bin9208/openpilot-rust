# Complete runtime startup integration boundary

This remaining work belongs to [issue 116](https://github.com/bin9208/openpilot-rust/issues/116),
[issue 120](https://github.com/bin9208/openpilot-rust/issues/120) and the complete
candidate gate in [issue 1](https://github.com/bin9208/openpilot-rust/issues/1).
The manager library from issue 106 is implemented; it is not yet an installed
manager executable with the complete daemon catalog and startup launcher.

## Existing components and remaining connections

`rust/crates/manager` provides initialization, lifecycle, ordered process
ownership, native startup/exit adapters and the continuous IPC loop. Its current
native example deliberately binds a synthetic child. The final executable must
bind each selected catalog entry to its actual packaged Rust executable,
arguments, working directory and persistent-process policy. Source Python module
names are provenance, never fallback launch targets. Keep a selected missing
implementation as an error and account for source-disabled entries explicitly.

The catalog's package/binary pair is insufficient as a launch binding. Current
entry points have these concrete composition requirements:

| Entry | Required binding |
|---|---|
| `card` | `--root` and `--numerics` are mandatory; bind the packaged checkout/DBC assets and verified numerical provider |
| `modeld`, `dmonitoringmodeld` | `--trusted-catalog` is mandatory; supply the correct immutable model catalog for each daemon |
| Encoder variants | Preserve each source mode argument such as `--stream`, `--carrot-vision-road` and the four YouTube choices while selecting the single native encoder executable |
| `controlsd`, `torqued` | Resolve their numerical companion directories; controlsd also reads torque assets relative to its working directory unless given `--assets` |
| Native subprocess users | Keep `openpilot-process-child` beside the executable, or pass the supported explicit launcher path |

This table comes from reading the current native argument parsers, not a
completed installed binding test. Resolve normal arguments independently of
fixture-only frame limits, fake providers and phase fences.

Managed diagnostic composition also remains open. The source Python launcher
adds the daemon log/Sentry tag and catches entry errors inside the child;
the source native launcher instead sets `MANAGER_DAEMON` and execs. The native
`managed-entry` library implements the former boundary, but the inspected
workspace manifests currently link it only from manager and Athena. A generic
parent wait cannot recover a typed error from another executable. Each
translated Python entry must therefore compose its actual child diagnostic
boundary, while retaining the source distinction for originally native entries.
Manager logging binds a process-local factory context; a fresh daemon
`Factory::for_runtime()` starts with an empty context. Do not assume exec
inherits the parent's bound fields as source fork did. Verify the installed
child's context and error event through the native collector and existing
upload path, rather than only checking manager logs.

The outer `manager.py` entry point still owns `helpers.unblock_stdout`, startup
failure reporting, stopping the UI before the error window, and final status.
The stdout wrapper uses a PTY, forwards SIGINT/SIGTERM, drains 4096-byte chunks,
drops undecodable/unwritable output, and reaps the child. Native integration must
retain these observable rules and launcher-lock ownership. Existing native
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
bundle at `<repository>/rust/native/brotli`. The bundle contains the encoder,
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
Native model provisioning and adapters for the remaining model-artifact formats
remain unfinished. Native readiness must use the verified `usbgpu-assets`
directory beside the selected runtime executable, consistently in modeld and
UI callers. A downloaded Python entry point or local ONNX chunkmanifest alone
does not establish native readiness. The final executable layout must provide
the same complete companion package to every caller. The cluster
encoder/decoder bridges remain project-owned conversion work.


The approved design permits Python model conversion during an explicit build
phase. The source ONNX path runs through `manager/build.py::build_usbgpu_model`
and SCons before manager startup; this does not require replacing the general
ONNX/Tinygrad compiler. The installed execution path must instead consume
verified native companions. Generic `run`, catalog `run_model` and local
`run_policy` artifacts need their corresponding native exports and queue/buffer
adapters; downloaded Python `make_input_queues`/`MODELD_INPUTS` or runtime warp
compilation cannot remain hidden execution dependencies.

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

This file records source inspection and pending integration, not successful
startup or device evidence. The user performs the first C3X comparison only
after the complete candidate passes its host/target build and composition gates.
