# Soundd daemon checker retirement preparation (#296)

Candidate for removal: only `rust/tools/check_soundd_daemon.py`. Keep the owned
PortAudio C ABI fixture, Rust WAV asset helper, native daemon, seven mode roster,
and their existing tests. No physical audio device is exercised.

The original-only root report is
`rust/crates/dev-tools/tests/fixtures/soundd-daemon/original-seven-f60.json`
(SHA-256 `53c8329d9aa7684deeefe987e5dd5662b307bb67cfdb50b8f3f55a6931e63a0f`).
It is copied byte-for-byte from `soundd-daemon/summary.json` in successful
[job 118914882870](https://github.com/bin9208/openpilot-rust/actions/runs/39505980261/job/118914882870),
artifact 12304571676, ZIP SHA-256
`23e074eef387fbff6bf1262993e0854536833183f5c4a593611bb6f78f8a8652`.
That job checked out source `f60e4ac0a655592aaef659f35cc354b86ffa09a0`;
the workflow head was `0ef32fb52e37758ce0740341ab598d77696be79f`.
The unchanged Python checker source SHA was
`fadc1ad42b382d4e4a6be128ead810e2b694e69b937525cb280e8f59cd799664`.
The 3,226-byte report includes all seven summaries. The other artifact members
retain raw original events, samples, packet inputs, stdout and stderr outside Git.

Required next cloud gate, before deleting the Python source:

1. Build one fresh native checker, daemon and WAV helper, and the owned C ABI
   library. Record all four hashes before and after. Preserve the existing
   native unit/process suite and strict Clippy. Keep the live original Python
   seven-mode run with the *same* daemon, helper and ABI library.
2. Construct two private source roots from the 21 pinned original input paths.
   One retains the exact original checker as a regular file; the second genuinely
   omits only that path. Require 21 and 20 verified pins respectively; missing,
   changed, dangling symlink, FIFO and reintroduced source must fail. Current
   source admission follows a symlink to an exact regular target, so this is
   not a general symlink-rejection claim. Run the *same*
   freshly built native checker ELF once per root, seven modes per invocation,
   using separate fresh output directories and unchanged 60/30/90-second
   bounded ownership and original per-mode waits. Do not stage a Python source
   fixture under another extension.
3. Run `cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools
   --features soundd-daemon-native --test soundd_daemon_retirement --locked`
   with mandatory `SOUNDD_ORIGINAL_OUTPUT`, `SOUNDD_NATIVE_PRESENT_OUTPUT`,
   `SOUNDD_NATIVE_ABSENT_OUTPUT`, `SOUNDD_NATIVE_PRESENT_ROOT`,
   `SOUNDD_NATIVE_ABSENT_ROOT`, `SOUNDD_NATIVE_PRESENT_EXEC`,
   `SOUNDD_NATIVE_ABSENT_EXEC`, `SOUNDD_DAEMON_BINARY`,
   `SOUNDD_DAEMON_WORKER_BINARY`, `SOUNDD_FIXTURE_ASSETS_BINARY`, and
   `SOUNDD_PORTAUDIO_LIBRARY`. It seals the original-only report, validates
   every fresh event trace and summary, compares stable report fields with the
   original observation, and requires identical native executable hashes and
   source maps differing by exactly the retired path.
4. Trace actual `execve` for each controller lane. The Rust reference test
   requires both native execution traces and rejects opaque or `execveat`
   attempts by counting all 22 openings before parsing each executable and
   argv. With the prebuilt ABI library,
   each native lane should execute checker CLI once, its same-ELF private worker
   seven times, WAV helper seven times, and soundd seven times (22 execs).
   Require exactly these hashed roles and no Python/shell/compiler in either
   native lane. Seal raw traces and all lane stdout/stderr/status and compare the same
   ELF and artifact hashes before/after. A trace should fail on extra, missing,
   or foreign children; it is not enough to inspect source strings or PATH.

After this gate succeeds at exact source and the artifact is independently
inspected, delete only the historical Python checker and its one workflow
invocation. Require the actual checkout source path to be absent at that new
head, retain the absent-root native seven-mode lane and execution audit, and
retain the sealed original-only observation. Do not count retirement or claim
current absence until this post-deletion cloud gate passes. The existing
`#433` finalization tests still require all four executable identities and all
remaining source pins through stdout, receipt publication and CLI teardown.
