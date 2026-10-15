# dev-md process crash evidence

Issue #338. Pinned source: `e8e152dbcfd9574a1791ed12026d1880e87689b3`,
`openpilot/system/manager/process_crash.py` and its tests. All validation uses owned
fixtures. No device access, external upload, production Params capture or reboot.

The supervisor reports unexpected nonzero exits before releasing their handles
and retries pending notifications on each ensure cycle. Successful exits and
requested stops remain unreported. A queue holds at most 32 notifications and uses
nonblocking local Unix datagrams. File access, Params, tmux, DNS and HTTP stay in
the independent diagnostic process.

Tombstoned owns the device-only collector. Separate capture and upload workers keep
capture responsive during a blocked upload. Capture precedes Params reads; exit
metadata survives unavailable Params or tmux. The port preserves the original
256 KiB UTF-8 tail policy, 20-report limit, duplicate snapshot protection,
60-second retry, and persistent acknowledgement for each destination. Uploads use
the saved text and existing DSM/Carrot Logs endpoints, without a settings dump.
A separate native upload process can be canceled and reaped during blocked HTTP.

## Evidence

- 106 cases executed against the pinned original match acceptance, callback order,
  complete metadata, and exact text bytes, including Unicode boundary truncation.
- Five host tests cover storage, acknowledgement and restart; real socket reception
  during blocked upload; socket permissions; worker cleanup; exact tmux arguments;
  missing or failed providers; the three-second timeout; and upload cancellation.
- An actual host HTTP test verifies session purpose, both endpoint paths,
  authentication, saved text, `FAKEUPLOAD`, and absence of a settings attachment.
- 18 supervisor library tests and three process/restart regressions pass.
- Strict all-target Clippy passes for manager, supervision and tombstoned, including
  the final capture-helper change described below.
- ARM storage, source comparison, socket reception, bounded sender and actual HTTP
  tests pass. All five collector/provider tests also pass with the actual ARM
  process-child helper, including missing-executable errors and timeouts.

## Capture boundary follow-up

The first ARM missing-executable test exposed an emulator boundary: `strace` shows
`execve` returning `ENOENT`, while the spawned child exits 127 and Rust's parent
receives a successful spawn. This changed the reported error category. The failing
receipt is retained. Capture now uses the existing native `process-child` helper's
exec-error handshake. No error expectation was relaxed. Telemetry CI explicitly
builds that already-packaged helper. The new integration passes fresh host and ARM gates with no skipped expectations.

Native panic/core diagnostics continue through Rust stderr and apport collection.
Python's `faulthandler` is not a Rust stack collector. Full fatal-native tmux/core
integration remains separate from the storage and transport tests above; source
coverage is not closed by this document.

## Managed error stderr follow-up (2026-10-16)

The pinned launcher prints the original exception before reporting. The native
launcher previously relied only on the logger inside the reporter and used an
environment-disabled backtrace. An actual blocked inert SDK child reproduced
this gap with both backtrace environment variables set to0. The launcher now
writes and flushes the complete native diagnostic before entering the reporter;
write failure cannot replace the original error. NativeException force-captures
the Rust stack at error construction. HOST and actual ARM child gates pass.

All affected host tests and strict all-target Clippy pass. Eight logging ARM
harnesses, including real inherited-context children, pass with an explicit
QEMU child runner. The first managed-runtime ARM run timed out receiving its
best-effort nonblocking log for the interrupt child; stderr was present. An
unchanged fresh execution passed both runtime cases. This transient is retained
as a transport/lifetime investigation; it is not silently counted as a clean
all-runtime run. Full fatal-native integration remains open.
