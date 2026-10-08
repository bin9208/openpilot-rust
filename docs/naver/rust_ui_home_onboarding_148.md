# Native Home and onboarding layouts (#148)

This slice ports the original large-display Home, sidebar, alerts, terms and
training guide, plus compact Home and onboarding. It preserves the MIT source
provenance in each layout. It is an intermediate UI/runtime conversion step for
issue #148 and the full-runtime gate in issue #1; it does not change production
daemon selection or establish vehicle behavior or CPU savings.

## Runtime behavior

- Home uses the existing native widgets for pairing and subscription content,
  the original update/alert priority, catalog ordering, message status rules,
  callbacks, release notes, scrolling, refresh thresholds and hit rectangles.
- Both Home variants read real Params and cereal messages through the shared
  native context. Unknown wire enum values retain the original fallback display.
  Compact Home keeps the original longitudinal-control gate and long-press timing.
- The large training guide loads the original nineteen PNG assets. A worker
  decodes CPU pixels into a bounded channel; the UI thread owns filtered GPU
  texture upload and release. Fast navigation uses the last available image until
  the requested image uploads. Dropping the guide closes the channel before
  joining its worker, and GPU textures remain owned by the renderer.
- Compact onboarding composes native cards, QR code, confirmation slider,
  navigation, live driver preview and driver-monitoring progress. It uses the
  real VisionIPC camera adapter and the original source textures. The original
  progress thresholds, gradual decay, sticky completion, RHD geometry and
  same-frame confirmation navigation are retained.
- Params writes, uninstall requests, reboot requests, brightness and interactive
  timeout changes remain typed effects. Tutorial destruction removes only its
  own inactivity callback. Initial accepted/trained flags remain cached, matching
  the original `completed` property rather than silently refreshing after writes.

The native external boundary still includes raylib/OpenGL, the narrow C++/CXX
rendering adapter, and the existing VisionIPC/msgq native transport. Source Python
is used only as the offline comparison oracle, never as the Rust layout runtime.

## Reproducible checks

The focused tools under `rust/tools/` compare actual source and Rust widgets,
using actual assets and isolated Params, IPC names and camera peers:

- `check_ui_home.py`: both languages, status and unknown enums, 80-second ping
  boundary, Home priority/actions/alerts, compact long press, all nineteen large
  training pages, record choice, restart, incomplete image loading and teardown.
- `check_ui_onboarding_cards.py`: every visible compact card, QR, navigation,
  accept/decline and record sliders, cancellation and incomplete confirmation.
- `check_ui_compact_onboarding.py`: complete terms-to-training-to-live-camera
  flows, both record choices, help/back recovery, inactivity, uninstall and
  cached-state/show/hide/close behavior. Every frame and state transition is
  retained; camera data comes from an owned original VisionIPC server.
- `check_ui_dm_progress.py`: original tutorial state/ring arithmetic at threshold
  edges and fractional navigation coordinates; native, aarch64/QEMU and Miri
  outputs are compared against the same inputs.
- `check_ui_training_adapter.py`: real-GL texture/ring resource exercise with
  AddressSanitizer and UndefinedBehaviorSanitizer. GPU driver leak reporting is
  disabled; this is not a claim to validate driver internals.

The current host evidence is kept outside Git in `.omo/evidence/ui-home-148/`,
with exact invocations, observed results, images and binary hashes in its receipt.
No C3X connection, vehicle test, real Wi-Fi operation or production service switch
is part of these checks. The user's first device comparison remains gated on the
complete project-owned runtime candidate and normal startup/log-upload path.

## Host implementation handoff (2026-10-01)

The retained final matrices contain 72 Home/sidebar/large-onboarding scenarios
(634 frame pairs), 22 compact-card scenarios (1,424 pairs), and 10 complete
compact-onboarding scenarios (3,280 pairs). A separate artifact verifier reads
every source/native state trace and PNG, checking dimensions, PNG signatures,
exact RGBA equality and discrete effects/Params/navigation outcomes. All 5,338
pairs match. The compact flow uses owned original VisionIPC peers; all 20 final
peer lifecycles exit zero through `stop`, with no socket remaining. All ten
native widget lifecycles retain zero callbacks and an empty navigation queue.

The 213 progress/angle/sticky/decay/geometry inputs match the actual source in
host, generic aarch64/QEMU, default Miri, strict-provenance Miri and tree-borrows
Miri lanes. Float64 progress uses the fixed 1e-11 bound; Float32 ring geometry,
colors and decisions match exactly. Real-GL ASan/UBSan exercises CPU decode,
all nineteen main-thread texture uploads, resource release and ring drawing.
Package tests, Clippy, formatting and focused Python lint pass. Four focused
examples build for host and generic aarch64; ARM graphical/device execution is
still outside this evidence.

Manual screenshot review covers every scenario, all compact cards, all nineteen
training pages and compact flow transitions in both language modes. The original
Korean large terms heading/body overlap, compact English-only copy/elision in
Korean font mode and first-frame zero-width alert buttons remain visible source
quirks. These are preserved parity, not claims of improved accessibility.

The first expanded compact run failed while a native peer started, before the
UI launched: `native test server did not start`, exit -6. That attempt lacked
the peer PID/prefix/syscall capture, so its cause remains undetermined. A passing
rerun is not a cause determination. Fresh isolated/concurrent startup diagnostics
exercise 140 starts using both the earlier and current peer binaries, including
40 killed-predecessor restarts; all reach readiness. The runner now retains each
peer's PID, prefix, socket, stdout/stderr, exit and cleanup metadata, bounds
response waits, and uses graceful stop. Failed startup capture is separately
verified with an intentionally failing peer, retaining the original failed run.

Exact scenario inputs, invocations, binary/source hashes, screenshots, failed
attempts and residual limits are indexed by `.omo/evidence/ui-home-148/receipt.json`
and `INDEX.md`. Parent independent review and full application/runtime integration
remain separate gates. This handoff does not close issue #148 or runtime issue #1.

Docs-Not-Needed: this is implementation-language parity for existing UI behavior;
no user-facing setting or user-guide behavior is added or changed.
