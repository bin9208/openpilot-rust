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

## Data and accessibility constraints

Parameters, typed cereal subscriptions, API workers and native effects retain
source timing and acceptance policies. English/Korean use the original translation
catalogs, fallback fonts and original localized copy; source elision or imperfect
layout is recorded rather than silently redesigned. Fixed screen geometry,
existing touch target sizes, visual contrast and lack of additional accessibility
controls are inherited constraints, not a claim of new accessibility compliance.

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
