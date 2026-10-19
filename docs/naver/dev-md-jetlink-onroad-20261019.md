# dev-md Jetlink ignition-independent connection (#400)

Issue: https://github.com/bin9208/openpilot-rust/issues/400
Base: `3ea790407566acee4c1d95ddbb6752453a4b8b24`. Target: **dev-md only**.

The Android App cannot connect during normal vehicle-powered startup because
Jetlink independently checks offroad in its daemon, USB owner, FunctionFS
setup/cleanup, and App selection/preparation. Earlier updater restriction removal
did not cover this path. The user explicitly requires removal of these gates.

## Design and implementation plan

1. Reproduce the ignition restrictions with gadget provisioning, owner lifecycle,
   FunctionFS setup/close/reconnect and App selection/HELLO tests. The initial
   run failed seven tests across four targets for the expected offroad gates.
2. Remove vehicle-state inputs from Jetlink transport and model preparation APIs.
   The daemon uses selected mode, physical attachment, exclusive resource ownership,
   link failure and reconnect time. Ignition transitions alone have no action.
   A closed/lost USB link releases ep0/UDC onroad and may reconnect after five
   seconds. Selecting Off and process shutdown also release owned resources.
3. Verify the affected host and ARM behavior, source policy oracle, PR and separate
   dev-md push checks; build and publish the exact-source AGNOS prerelease and
   verify the public package using the existing ARM installer and launcher.

USB/eGPU/controller ownership and root helper ownership remain enforced.
Selected model SHA, contract, fresh loaded identity, protocol, finite outputs,
frame/sequence/deadline checks, control source acknowledgement and loss latch
remain unchanged. No new automatic control activation is introduced.

CPU bounds remain: daemon sleeps 500 ms per iteration; link recovery waits five
seconds; idle App state requests are limited to once per two seconds; preparation
polls retain deadlines and physical disconnection cancellation. The original
ownership reference is compared with its ignition gate satisfied, documenting the
intentional policy change while retaining its eGPU/idempotence/error checks.

No paired user document changes are authorized. This focused regression uses
this investigation record and tests. No physical device connection or measured
CPU result is claimed. Athena WIP and #296 tool conversion are outside scope.

## Delivery evidence

Focused host verification: **79 tests pass**, Clippy all targets with warnings
denied passes, and the source oracle passes 40,000 transition decisions, 500
ownership operations, 500 adapter frames and 20 parser frames (186 cases).
The Android first-connection test waits for App selection and asynchronous
engine readiness with an empty saved selection. Disconnect cancellation and
two-second idle throttling are also covered.

Delivery in progress. Exact commits, Actions and release/installer verification will be
recorded in issue #400. Physical Android/device acceptance remains user-owned.
