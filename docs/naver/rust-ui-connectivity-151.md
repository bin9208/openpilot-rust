# Shared UI and connectivity integration (#151)

Tracks [#151](https://github.com/bin9208/openpilot-rust/issues/151) under the
[full runtime port](https://github.com/bin9208/openpilot-rust/issues/1), after
estimator integration #147. Components are shared UI #125 at `bb6ef7d9`, its
included Wi-Fi #135, and discovery reporting #142 at `dbf38b60`.

## Scope

The shared framework owns application/widget state, rendering policies,
translations, forms, navigation, network screens, input, recording and graphics
lifecycle. Wi-Fi owns typed commands/snapshots and private D-Bus connections to
the original NetworkManager protocol. The discovery daemon owns IP selection,
identity, reporting, heartbeat/retry and HTTP behavior. Neither the product UI
#148 nor the separate Carrot server is declared complete by this integration.
The product `ui` catalog entry remains unported; production selection is unchanged.

External dependencies remain explicit: raylib/EGL/GLES, font data, ffmpeg,
NetworkManager/libdbus, Linux input/networking, and HTTP/TLS libraries. New locked
registry entries match the already tested UI/Wi-Fi component lock exactly;
existing external versions are unchanged. Original source/license provenance is
retained, and native product code does not launch the Python source oracle.

## Required evidence

`rust UI and connectivity` joins the required `rust checks` aggregate. The job
uses owned Xvfb, D-Bus, HTTP and Params fixtures and retains results/images even
on failure. Its stages include:

- All 13 shared framework gates: widgets, scrolling, navigation/stack, scrollers,
  rendering, forms, networking, polygons, diagnostics, real application input,
  FPS and EGL lifecycle.
- All source PO catalogs/plural rules and color-emoji raster equality.
- Existing startup UI source/native rendering, live input, owned children and
  instrumented native adapter checks.
- Wi-Fi policy and actual original/native private NetworkManager protocol.
- Discovery state/HTTP, six original/native CLI scenarios and address ABI checks.

The parent inspected Korean, emoji, keyboard and network captures from the final
shared framework evidence. Each source/native PNG pair is byte-identical. The
final forms-runner refactor retained the inspected keyboard hash. These component
artifacts establish host behavior only; the combined exact-SHA CI run is a
separate required result.

Reproducible component commands and precise limits are in
[shared UI](rust_ui_framework_125.md), [Wi-Fi](../rust-port/wifi-validation.md) and
[discovery reporter](../rust-port/cweb-push-validation.md). The EGL test uses an
owned ABI fixture, not a physical DMA-BUF. Prebuilt external graphics drivers are
not sanitizer-instrumented. CI includes GNU/musl ARM builds but cannot establish
AGNOS execution, real Wi-Fi/display operation, CPU savings or complete startup.

Exact-SHA Actions and separate post-merge results remain integration gates. Full
normal startup and the existing log-upload path remain open under #1. The first
device comparison remains the user's step after the complete runtime candidate.

Docs-Not-Needed: shared native foundation and validation wiring; no user-visible
setting, public guide behavior or production selection change.
