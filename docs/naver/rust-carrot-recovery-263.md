# Standalone recovery runtime: issue 263

The standalone recovery port is being implemented under
[#263](https://github.com/bin9208/openpilot-rust/issues/263), separately from the
main web application in #225. It preserves the existing port 6999 entry point,
recovery frontend assets, independent persistent terminal, Git/tools actions,
CwebPush recovery boot and support-session interfaces. Runtime code is Rust;
the original Python implementation is an offline comparison oracle and the
source of frontend strings extracted during the Cargo build.

## Resume after power interruption, 2026-10-12

The existing uncommitted recovery bootstrap was retained on
`codex/feat-263-carrot-recovery`, based on `d63f5d2f8`. No vehicle was contacted.
Disk checks showed 151 GiB before the first build and 150 GiB after the new
bounded package builds. Incremental compilation was disabled and two Cargo
jobs shared `rust/target`.

The following are intermediate host results, not a completed runtime or device
handoff. Exact invocations, binary hashes and captured requests remain in the
local evidence directories; later edits require applicable fresh checks.

| Boundary | Observed evidence |
| --- | --- |
| Bootstrap, static assets, HEAD, request/header limits, Expect/Continue and JSON boundary/error behavior | 25 original/native pairs passed. Empty Content-Length and integer/UTF-8 error differences were reproduced and corrected; exact 64 KiB JSON acceptance and over-limit rejection were observed. |
| Support idle/stop and absent command approval/rejection | The original returned 200 for stop while the old native binary returned 404; the integrated implementation passed all 17 HTTP pairs. |
| Upload destinations and wire contents | 16 original/native pairs passed, including session/token authorization, denied/missing session, partial/all errors, disabled webhook, malformed URL, missing log, 8 MiB Discord truncation, 301/307 responses, 1 MiB response boundary and invalid UTF-8. Every recipient was an owned loopback server. |
| HTTP tmux upload action | Three actual HTTP actions passed success, partial failure and complete failure comparisons, including captured replacement UTF-8 bytes, destination requests and the downloadable file field. |
| CwebPush status/enable/boot | Eight owned-process checks and 14 original/native comparisons passed with actual Params aliases and owned HTTP recipients. The 2-second stabilization and 5-second retry were observed; the full 120-second unavailable deadline has not been observed. |
| Persistent PTY and WebSocket | Actual original/native peers passed framing, prefetched input, ping/pong with fragmentation, binary/raw input, fixed 100 columns, row resize, same-PID reconnect, variable persistence, 512 KiB replay, clear, broadcast, exit status and client close. Login/MOTD commands were compared as arguments to an owned no-profile shell provider. |
| Git recovery | 36 cases across owned repositories and external command providers demonstrated source/native read and repair behavior, inherited transaction lock/PTY descriptors, stale-index refusal while Git is active, removal during a quiet control, real tmux capture and command side effects. A final PID-namespace run removes interference from unrelated host Git processes. |
| Support session composition | 13 actual source/native scenarios passed: duplicate start, PIN rejection/authentication, owner absence, command approval/rejection/15-second expiry and real shell effects, controller/viewer handoff, guest static/CSP restrictions and session-stop cleanup. Only owned webhook and source-supported fake tunnel recipients were used. |
| Support external providers | Eight checks passed with source message comparisons, owned Git/Params metadata, loopback executable download, output draining, failed/silent tunnel launch and 25-second deadline cleanup. |
| Shutdown while tunnel startup is pending | The old ordering blocked beyond three seconds; closing support before joining request workers exited in 0.034 seconds and reaped the owned tunnel child. |
| Standalone host package | The normal executable, process launcher and frontend assets were copied into an owned package without the main web executable or Python files. All 25 HTTP comparisons, help, invalid argument, duplicate-listener refusal and clean shutdown passed. |
| ARM64 / pinned AGNOS loader | Both GNU ARM64 binaries built. Compressed, sparse and raw image hashes matched the pinned 19.8-carrot-bt1 manifest. Five freshly extracted providers satisfied DT_NEEDED, symbol versions and strong imports; the image loader returned zero for both `--list` invocations under QEMU. The final recovery rebuild after Unicode and tunnel cleanup fixes again passed the ABI check and its actual `--help` exited zero through that loader under QEMU. This checks loading and the CLI entry, not normal AGNOS runtime or hardware behavior. |

Local evidence roots: `.analysis/scratch/2026-10-12-recovery-resume/` and concise
worker receipts under `.omo/evidence/recovery-263-terminal/`,
`.omo/evidence/recovery-cwp-2026-10-12/` and
`.omo/evidence/carrot-recovery-263/`.

## Ownership findings and remaining gates

The first native lifecycle control found that an interactive Bash background
job used a different process group in the same session and survived leader-group
cleanup. This is tracked separately in
[#270](https://github.com/bin9208/openpilot-rust/issues/270). It is distinct from
the unreaped-leader identity issue #265 and the original reset race #264. A
recovery-only child-subreaper fix now reserves the session leader and reaps only
same-session adopted children after non-reaping ownership observation. Actual
24-reset, active-client replacement, leader-exit, two-level resistant descendant
and shutdown controls passed; an unrelated child remained alive and independently
reapable. No pidfd-only target requirement was introduced. The original live-reset
stress did not reproduce #264 in this run. Main-server adaptation remains separate.

The final bounded host run passed 21 recovery and 19 process-supervision tests,
and Clippy passed with warnings denied. Two ignored tests are subprocess drivers
invoked explicitly by the source comparisons, not omitted behavior checks.
The source comparisons are wired into the required workspace CI job; remote
exact-SHA execution is still pending.

The full 900-second support session lifetime has not been observed in this run;
command expiry was observed at its real 15-second setting. Launcher
repository-lock handoff/recovery ordering, watchdog integration, exact-SHA CI
and normal startup/log-upload composition remain gates under #269.

The inspected host recovery ELF currently requires only the normal loader,
libc, libm, libgcc_s and libstdc++; it does not link the main web executable,
FFmpeg, camera, graphics or model providers. The separate pinned-image ABI
receipt is `.omo/evidence/recovery-cross-2026-10-12/evidence.md`.
The final recovery ARM64 SHA-256 is
`819d1661e6b40c4c9d7a26a7d88fd4b7e9c4c8369b4e50ff372e029429c81c8d`;
process-child remains
`03f57a6efbb6c061ab4c42841cc4655ddb8ddffaec5e8d2ba89b2c9b6a4aeb7e`.
External bash, Git, tmux, cloudflared,
certificate stores and OS services remain explicit providers.
