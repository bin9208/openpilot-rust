# Runtime metadata, telemetry, crash and alert integration

Issue [#83](https://github.com/bin9208/openpilot-rust/issues/83) follows
[supporting runtime integration #71](support-runtime-integration.md), under the
approved [full-runtime design](design.md). This branch remains an intermediate
implementation. Production daemon selection and the first device-test gate are
unchanged: complete the entire project-owned runtime, normal startup and the
existing log upload path first.

## Component inputs

| Component | Reviewed input | Evidence status |
| --- | --- | --- |
| Runtime build metadata and cached Git helpers #73 | `97edf4361e145619259407426e621b0387fa54e9` | [Source/API contract and limits](version-validation.md); parent independently passes all 171 source comparisons and the actual native collector path, and verifies all 22 source/executable hashes |
| Statistics producer and continuous daemon #75 | Pending final review | Component implementation and source/runtime validation in progress |
| Tombstone daemon and native crash-reporting policy #76 | Pending final review | Component implementation and source/local-transport validation in progress |
| GPIO alert beep daemon #79 | `a86f99d14f35eaa6e544a9c346d338698a29fed8` | [Source/runtime contract](beepd-validation.md); parent independently passes 17 daemon scenarios and four CLI cases using pinned pycapnp 2.1.0, and verifies all 555 source, binary and artifact hash references |

The metadata merge retains both timed's `string_fields` helper and the immutable
JSON views. Cargo resolves the combined lockfile from the existing integration
lock: the only new registry package is the component-pinned Unicode 15.0 data
crate `unicode-general-category` 0.6.0. Existing dependency versions remain intact.
The initial combined metadata/JSON/time crates pass formatting, warnings-denied
Clippy and all 19 focused Rust tests at `dc219261`. Later component integration
requires its own applicable validation.

Supporting runtime PR #80 merged at `96decc71d6fc7505f62b508f381bb0c65cc26e9d`
after its exact-head Rust, integration, fast and mapped-documentation checks passed.
This branch includes that merge. A separate required telemetry CI job builds the
new binaries, compares metadata and alert behavior with the original source, and
retains raw IPC and Params evidence. Statistics and crash checks are added after
their component review is complete.

The first local metadata collector run used historical pycapnp 2.2.4. Its packet
observations remain recorded, but it is not the pinned dependency validation.
The integrated CI and the standalone script dependency declarations use the
source-required pycapnp 2.1.0 and pyzmq 27.2.0; combined local checks use that same
isolated dependency environment. No performance conclusion uses the earlier run.

Inherited source issues [#77](https://github.com/bin9208/openpilot-rust/issues/77)
(apport filename shell interpolation) and
[#82](https://github.com/bin9208/openpilot-rust/issues/82)
(untranslated Params integer exceptions) remain separate. The native ports must
record their deliberate safety/diagnostic differences rather than describe
those source defects as fixed upstream.

Combined checks, exact-head Actions and post-merge results remain pending.
Generic ARM artifacts, local IPC/filesystem tests and simulated crash/GPIO
fixtures do not establish hardware, startup, device or CPU acceptance.

Docs-Not-Needed: internal runtime integration and engineering evidence; no selected
production behavior or user setting changes.
