# Native startup UI — issue #117

The `openpilot-startup-ui` crate implements the startup Spinner and TextWindow
surfaces and their owning child wrappers. Source provenance is
`openpilot/common/{spinner,text_window,network_info}.py`,
`openpilot/system/ui/{spinner,text}.py`, and their application, text, button,
scroll and mouse dependencies. Existing project licensing remains in force.

The Rust code owns text/progress state, Unicode numeric handling, source wrapping
and button word breaking, geometry, scrolling, IP failure grace, stdin updates,
140 Hz board touch polling, and child lifecycle. A CXX adapter calls the existing
external raylib renderer. It preserves the source TextWindow wrapper's unusual
exit-code-1-only `wait_for_exit` contract; the PC UI button itself exits zero.
Source viewport size, BIG-specific font scale, scaling, Korean fallback fonts,
texture assets, and recovery IP display are preserved. See the crate's DESIGN.md.
The six-line change to process supervision adds inherited-output/piped-stdin
launching for the native spinner. Normal production daemon selection is unchanged.

## Native dependency staging and portable build boundary

The repository's historical raylib archive is 5.6-dev; it is not the dependency
used by the current Python UI. Use the `uv.lock`-pinned
`comma-deps-raylib==6.0.0.1.post103` native archive. The staging helper checks the
wheel SHA256, extracts only headers/native archive, and optionally builds a GNU
shared plugin with a version contract. It never imports or executes Python UI
code. Python is a build/validation dependency only.

Host staging (requires g++, GL and X11 development libraries):

```sh
python3 rust/tools/stage_startup_ui_raylib.py --arch x86_64 --output "$RUNNER_TEMP/startup-raylib-host" --shared
export STARTUP_UI_RAYLIB_ROOT="$RUNNER_TEMP/startup-raylib-host"
export STARTUP_UI_RAYLIB_LIBRARY="$STARTUP_UI_RAYLIB_ROOT/lib/libopenpilot-raylib.so"
```

GNU aarch64/AGNOS plugin staging uses the comma DRM backend and a matching GNU
cross compiler with aarch64 EGL, GLESv2, DRM and GBM development libraries:

```sh
python3 rust/tools/stage_startup_ui_raylib.py --arch aarch64 --output "$RUNNER_TEMP/startup-raylib-arm" --shared --cxx aarch64-linux-gnu-g++
export STARTUP_UI_RAYLIB_ROOT="$RUNNER_TEMP/startup-raylib-arm"
```

The helper emits `SOURCE_SHA256` and `plugin-build.json` (source URL/hash,
architecture and exact linker command). Package the resulting
`lib/libopenpilot-raylib.so` with the GNU runtime and retain its required board
EGL/GLES/DRM/GBM shared dependencies. Runtime lookup uses
`STARTUP_UI_RAYLIB_LIBRARY` or the system loader's `libopenpilot-raylib.so` path.
Missing or incompatible plugins fail explicitly; there is no old archive,
Python, fake renderer or no-op fallback.

Both GNU and generic musl Cargo builds compile the same actual `dlopen` adapter;
Cargo links only its platform dynamic-loader API, not the GNU raylib archive or
GL libraries. Header-only staging for the musl compilation gate is:

```sh
python3 rust/tools/stage_startup_ui_raylib.py --arch aarch64 --output "$RUNNER_TEMP/startup-raylib-headers"
export STARTUP_UI_RAYLIB_ROOT="$RUNNER_TEMP/startup-raylib-headers"
cargo zigbuild --manifest-path rust/Cargo.toml --workspace --release --target aarch64-unknown-linux-musl
```

A generic musl build is loader compilation evidence, not proof that the GNU
plugin runs inside a musl process. Device candidate packaging uses the GNU/AGNOS
plugin. Parent integration retains both existing workspace build gates.

## Focused host evidence

Artifacts are in the issue worktree's `.omo/evidence/startup-ui-117/`.
`manifest.md` records exact scenarios, invocations and observed results.

- Native/source state matches for six scenes: small, large, wrapped and scaled
  spinner; English error text with wheel scroll; large Korean error text.
- English and Korean TextWindow screenshots are pixel-identical. Spinner
  screenshots have 38/143/38/64 changed pixels respectively, confined to rotated
  texture edges; these are reported rather than called pixel equality.
- Actual native PC TextWindow exits zero after an XTest button click; actual
  spinner processes stdin status/progress and exits zero on SIGINT.
- Actual native child wrappers launch, update, stop and reap spinner, and launch,
  poll and terminate TextWindow. Seven contract tests exercise retry/replay,
  Unicode digits, wrapping, IP loss grace, slot input, scrolling and button reentry.
- AddressSanitizer checks the project CXX adapter's repeated create/render/capture/
  destroy lifecycle and duplicate-window rejection. External prebuilt raylib is
  not instrumented; driver leak checking is disabled. Missing and incompatible
  plugin invocations exit one with explicit load/contract diagnostics.

Shared application debug overlays, recording, burn-in and profiler tooling belong
to the broader UI runtime conversion, not these startup widget surfaces. Host
screenshots, mock board events and GNU dependency staging do not establish AGNOS
rendering, physical touch input, actual board reboot, or vehicle acceptance.
No device was contacted, and no complete-runtime readiness or CPU saving is claimed.
