# Product UI source contract

This is a source-preserving native port of `openpilot/selfdrive/ui`. The existing
Python/raylib implementation, original assets, and shared Rust UI primitives are
the reference. No visual redesign, new responsive breakpoints, or setting behavior
is authorized. `../startup-ui/DESIGN.md` defines viewport, fonts, asset scaling,
input, and native renderer conventions. Source geometry and motion remain exact;
web/React/Lighthouse tooling does not apply to this native raylib application.

## Components and states

Big list settings use the source `ListItem`, action buttons/toggles, HTML
expansion and legacy vertical scroller. Compact settings use the existing
402×180 big buttons, 180×180 circle buttons, 360×180 device information, horizontal
scroller and navigation dismissal. Text, icons, opacity, spacing, typography,
filters and enabled/visible rules are ported from the corresponding source
classes. Compact input retains the 520×170 keyboard background's explicitly
stretched geometry and the original text/gradient/cursor composition.

Model/road rendering, cameras, onboarding, home/sidebar and remaining settings
must follow their own original source modules and real interaction state. They
must not be replaced with screenshot assets or simplified placeholder controls.

## HUD contract

Both HUDs retain the original source palettes, licensed speed/wheel/turn textures,
unit conversions, set-speed availability and persistence, cruise popup motion,
eGPU badge priority, navigation-provider labels and gear/hold/driving-mode states.
The compact TurnIntent uses 50x20 textures, alpha RC0.05 and rotation RC0.1 at
the current UI frequency; pre-lane-change starts at30degrees and lane-change
rotates out in the remembered direction. Styled HUD text retains the original
eight45degree outline offsets, optional shadow, font fallback/scaling and anchor
measurement. Text coordinates remain Float64 until final raylib Float32 narrowing.
Display-only deceleration/navigation presentation preserves source normalization,
provider prefixes, eight-character fallback labels, lifecycle priority and color
modes; it does not modify detection, selection or vehicle-control behavior.

## Data and accessibility constraints

Parameters, typed cereal subscriptions, API workers and native effects retain
source timing and acceptance policies. English/Korean use the original translation
catalogs, fallback fonts and original localized copy; source elision or imperfect
layout is recorded rather than silently redesigned. Fixed screen geometry,
existing touch target sizes, visual contrast and lack of additional accessibility
controls are inherited constraints, not a claim of new accessibility compliance.

## Native application ownership

`openpilot-ui` owns the native application, both Main layouts, the original
28-service subscriptions, native Params namespaces, API/Wi-Fi/eGPU workers,
onboarding and settings navigation, display/brightness, recording and bookmarks.
State updates and action processing follow widget rendering and also run when
the display skips rendering. The UI remains20Hz; scheduling retains the approved
onroad core6/nice19 policy and offroad restoration, including the encoder child.
Resource fields keep the GL application alive until widget textures are dropped;
normal exit and error exit stop the recording child and flush queued Params.

Large Main processes sidebar settings callbacks before rendering its content.
Its content rectangle is cached eagerly when the rectangle changes, preserving
the source's existing geometry even across sidebar visibility changes. Compact
Main preserves the2.5-second onroad delay, plot/cluster selection, standstill
departure and timeout rules. Offroad timeout immediately pops navigation and
separately retargets home, as the original immediate pop discards callbacks.

Augmented road rendering owns calibration, camera preparation, projection,
model/HUD/driver/alert layers and the original traffic/confidence/vision effects.
Calibration retains the original camera tables, Euler composition, zoom and
offset cache keys, and compact DevicePosition writes. Native Params float reads
retain float32 prefix parsing through the borrowed CXX boundary. TrafficLight
uses the existing native Python-compatible JSON/numeric parser, including
arbitrary integers and Unicode digits; no Python interpreter executes in UI.

The source-required shared dependencies are `Params::for_runtime_at` for the
memory namespace and `hardware_info::parse_float` exposing the existing parser.
The production launch needs sibling `openpilot-process-child` and
`openpilot-updated` executables plus the existing raylib/msgq/native dependencies.
Manager selection and full-candidate startup/upload integration remain parent
work; desktop fixtures substitute external hardware/Wi-Fi/eGPU data boundaries.

## Verification and ownership

Host QA renders the original and native widget trees through the real pinned
raylib/GL adapter, uses owned Params/IPC/clock/effect boundaries, compares exact
pixels and discrete outcomes, and retains scenario inputs and source/native
captures. Calibration/camera/model computations need their declared numerical
bounds in addition to screenshots. Background requests own their worker data;
callbacks use weak widget ownership where they reference page lifetimes.
Only the completed application and complete runtime startup/upload path can
qualify for the user-performed first device comparison. Host evidence does not
establish C3X/AGNOS/vehicle behavior or CPU savings.
