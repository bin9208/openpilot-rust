# dev-md bounded native reboot helper

Scope: issue #338; source `ajouatom/openpilot:carrot-wip` at
`e8e152dbcfd9574a1791ed12026d1880e87689b3`, `openpilot/common/reboot.py`.
All work stays on `dev-md`. No device, real audio output or real reboot was used.

## Implemented and verified helper

`openpilot-reboot` separates the sound child from the reboot request. Device
admission precedes spawning; sound has a four-second deadline and failure is
best effort, while `sudo -n reboot` has a fifteen-second deadline and propagates
failure. Asynchronous requests own a new session. Delay follows the source
zero-to-five-second clamp, including infinities and NaN. The helper is named
`openpilot-reboot` to avoid recursively resolving itself as the system `reboot`.

The native sound path reads the raw volume setting, normalizes the fixed PCM16
prompt and prepends 150 ms of silence. PortAudio is dynamically loaded only in
the sound child. The parent imports no audio provider. Fixed asset identity is
asserted before comparing PCM; this is not a claim about all malformed WAVs.

Validated on HOST and QEMU ARM:

- 26 source volume cases and nine valid WAVs: rate, channels, length and all
  output Float32 PCM bytes match the original NumPy/sounddevice boundary.
- 32 policy combinations and delay edge cases preserve admission, sound failure
  notices and ordering. Actual owned missing/error/timed-out children still reach
  the fake reboot recipient; new-session identity and child reaping are checked.
- Ten fake PortAudio ABI cases exercise normal output, underflow and initialization,
  open/null stream, start/write/stop/close/terminate errors. The fake provider is a
  Rust cdylib. PCM bytes and resource cleanup order are recorded. No real device
  was opened.
- Host all-target strict Clippy and host/ARM helper links passed.

Receipts: `.analysis/scratch/2026-10-15-dev-md-resume/reboot-*.log`,
`reboot-native/ffi-host.json`, `reboot-native/ffi-arm.json`.
Fixtures: `rust/fixtures/reboot-sound-final.json`,
`rust/tools/fixtures/reboot/portaudio.rs`.

## Verified caller integration

The board hardware adapter (including manager/startup UI users) and web system/
tools requests now select the helper. The package sealer, installer and startup
verifier require it. Caller checks passed: 35 hardware-policy cases, 24 web action/default/calibration
cases, manager exit adapter composition and 13 owned bootstrap scenarios on both
HOST and ARM. The ARM bootstrap executes the actual ARM launcher under QEMU with
host-owned fake command recipients; the board adapter additionally uses the real
ARM process-child helper. These are not complete daemon or hardware runs.

Package sealer/installer/launcher tests passed 9 + 46 + 2 cases on HOST. Strict
all-target Clippy passed for hardware-control, carrot-server, manager,
runtime-launcher, runtime-installer and dev-tools. The broader check also caught
and repaired a production-item/test-module ordering lint in the native HCQ tool.
The ARM manager example needs the explicit target FFmpeg library search path;
its linked adapter run passed after correcting that validation command.
The hardware source oracle explicitly composes the pinned final reboot method
with this branch's existing board policy and observes the separately verified
helper boundary; it does not claim whole-file raw upstream identity.

The pinned recovery web server and AGNOS shell path still use direct reboot;
those are separate source paths, not silently treated as helper callers.
Full runtime packaging, startup recovery/rebuild behavior and exact-commit CI
remain pending. Helper gates do not establish complete runtime or device readiness.
