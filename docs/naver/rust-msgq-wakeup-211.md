# Message-queue notification wait (#211)

The native planner in the [d1cb4516 host comparison](https://github.com/bin9208/openpilot-rust/actions/runs/37169141943/job/111338517789)
published one extra model-triggered longitudinal plan in each run. Recorded
live-to-model publication gaps were approximately 101 ms, crossing the unchanged
100 ms liveTracks fallback threshold. Pure policy and main-loop comparisons
passed. The failed raw messages and timings remain in the CI artifact.

Investigation found a notification race in the shared queue wait: SIGUSR2 could
arrive after the last empty-queue check but before nanosleep began. The no-op
handler consumed the signal, then the reader slept until its timeout. The
inherited C++ implementation contains the same non-atomic sequence. The CI
timings motivated this investigation; they are not a syscall trace proving the
cause of every historical delay.

## Change

The native Rust poll now blocks SIGUSR2 while examining queues. `ppoll` atomically
restores the caller's signal mask during the wait, so an intervening notification
remains pending until the wait begins. The mask is restored on success, error and
unwind. Other signals and a caller's pre-existing SIGUSR2 block are preserved.
Finite waits use the original requested timeout as a monotonic deadline across
interruptions. Infinite polling retains periodic 100 ms queue/reconnect checks.

This uses safe APIs from the existing locked nix 0.31.3 dependency and removes
the direct unsafe nanosleep call. No queue layout, message ordering, planner
policy, validity threshold, process priority or CPU placement changes.

## Evidence

- A controlled signal delivered immediately after an empty check reproduced a
  200 ms wait before the fix. The same test passed after the change.
- Caller-mask restoration on ready/error/unwind and zero-timeout behavior passed.
- All `openpilot-msgq` native test targets passed, including original C++ peers,
  wrap/overflow/eviction/restart, corrupted memory, queued transport and VisionIPC.
- The existing interruption regression still requires ten signal-handler
  acknowledgements and a 120 ms receive timeout. Its syscall observer now also
  recognizes ppoll; no acknowledgement or timeout assertions were removed.
- Strict Clippy for msgq and planner and workspace formatting passed.
- The unchanged real planner IPC comparison passed four fresh daemon runs:
  121 publications each, 40 valid longitudinal plans, 39 valid lateral plans and
  39 valid assistance messages. Each had six model triggers, 35 liveTracks
  triggers and 34 fast overlays, with no policy-field differences.
- A separate read-only parent-requested review found no actionable issue in the
  signal-mask, timeout, cleanup or test changes.

The executed planner ELF is SHA-256
`596b91bc863f1ed9d5716b8ca524697f2ffdb130de9789397eb2a54d87ff6971`.
Both RED/GREEN test ELFs, source capsule, command receipts and real IPC messages
are retained under `.omo/evidence/msgq-wakeup-211/`. The capsule includes a
declaration-order-only rustfmt correction after the host ELF build.

Signal syscalls require native execution; pure Miri coverage must not be described
as proof of kernel signal delivery. Corrected hosted checks, including ARM, remain
required before closing #211. No device performance or driving result is claimed.
