# Native staged updater (#118)

The `openpilot-updated` binary and `rust/crates/updated` port
`openpilot/system/updated/{updated,process,common}.py` and the required
`openpilot/common/markdown.py` release-note renderer. Source provenance is the
unchanged code at integration base `bb4210331a7a09d969cf3ca791748106dc3fe861`.
The binary does not invoke Python. Python is used only by the original-body
comparison and owned command fixtures.

The daemon retains the exclusive staging lock, DisableUpdates startup exit,
low I/O priority, install timestamp, initial consistency invalidation, initial
60-second/time-validity wait, 300-second failure retry and 5,400-second success
wait. SIGUSR1 requests a check and SIGHUP requests a fetch. The three-day
metered-network rule, branch ordering/exclusions and tizi release mapping,
ordered Git fetch/reset/submodule work, copy/finalize flags, LFS-prune failure
handling, descriptions/release notes, Params and connectivity alerts retain the
source behavior. Full Git repacking is not added.

The manager retains the offroad/SoftwareMenu predicate; this daemon does not
add a second IsOnroad policy. Its manager catalog entry advertises an isolated
native candidate, without changing production selection. Commands run in owned
sessions with merged stdout/stderr and the original appended Git maintenance
configuration. SIGINT/SIGTERM termination kills/reaps that command group even
when its leader has exited and a descendant still holds the pipe. Existing
process-supervision launch modes remain unchanged.

`--basedir` selects the checkout; `UPDATER_STAGING_ROOT` and `UPDATER_LOCK_FILE`
retain the source path overrides. Default system paths and sync behavior are
native. `--system-root` redirects paths and limits validation syncs to the owned
root; it is not a command sandbox. The validation PATH supplies a strictly
owned overlay-command fixture, while Git actually fetches, checks out, resets
and finalizes temporary repositories. No real sudo, mount, host update,
AGNOS-slot flash, reboot, production selection or C3X access is performed.

## AGNOS boundary and remaining acceptance

`Agnos` is a typed integration interface exposing target-slot selection and
`flash_agnos_update(manifest, target_slot)`. The updater preserves the current /
requested OS-version comparison, consistency invalidation, NeosUpdate alert and
c3/tici manifest selection. Integration #120 links the project-owned AGNOS #119
implementation directly, preserving background casync selection and bounded
network retries (`standalone=false`, `retry_network=false`). Native errors leave
the checkout inconsistent and the alert set. The adapter uses the existing
native logger, child launcher and harmless path overrides for owned fixtures.
`--agnos-config` uses the AGNOS typed config; its default paths address the device
partitions independently of `--system-root`. Background flashing does not switch
the active boot slot; normal startup retains verification and swap ownership.
Automatic verified inactive-slot update behavior is not replaced by an approval
prompt or bypass.

Native dependencies are Git (including submodules and LFS), bash, find, sudo,
mount/umount/chmod/rm, util-linux ionice, Linux filesystem/xattr/sync/locking and
process APIs, plus the existing captured-child helper and logging dependencies.
Physical OverlayFS mounts, block devices, AGNOS installation and vehicle
acceptance remain unvalidated here. A daemon port does not satisfy #1's complete
normal-startup/existing-upload device handoff.

Focused host commands and artifact descriptions are in
[updated validation](../rust-port/updated-validation.md). Parent integration
owns exact-SHA Actions host/aarch64 checks and the complete startup composition.

Docs-Not-Needed: isolated runtime candidate; no selected setting or user behavior changes.
