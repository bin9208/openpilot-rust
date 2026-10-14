# Queued exec error after helper exit (#329)

Issue: https://github.com/bin9208/openpilot-rust/issues/329

Source `dd9343cb9cdc395020280cbe85f1e65920fa9872` failed the updater's
`missing_executable_is_visible_start_failure` assertion in
[workspace CI](https://github.com/bin9208/openpilot-rust/actions/runs/38576054408/job/115894734046).
The other twelve UI library tests passed. This is a native process handshake
race, not a reason to relax the original updater error contract.

The parent can receive EAGAIN from nonblocking `accept`, then observe helper
exit after the helper has connected and queued its four-byte exec errno.
Previously it returned `helper exited before exec handshake` immediately.
It now makes one final nonblocking accept after observing exit, consuming a
queued errno/EOF through the existing parser. A final EAGAIN still reports an
absent handshake. NOWAIT retains the leader for the existing cleanup/wait;
no retry, reboot, startup selection, or accepted-stream timeout policy changes.

Original `0de1a940:openpilot/system/ui/tici_updater.py:74-84` catches
`subprocess.Popen` errors and displays `Unable to start updater: {e}`. An owned
Python Popen missing-path probe confirmed the `[Errno 2]` prefix. The Rust
assertion remains strict and now prints its actual failure if it regresses.

## Local evidence

- The unchanged focused test and 1,000 ordinary repetitions passed. This does
  not disprove the race.
- Deterministic RED: run the focused test binary under
  `strace -f -qq -e trace=accept4 -e inject=accept4:delay_exit=50ms:when=1`.
  Actual failure was `Unable to start updater: native child launch protocol:
  helper exited before exec handshake`. This delays the return of real EAGAIN,
  allowing the queued error and child exit to occur before waitid.
- Identical injection with the fix: PASS, retaining the `[Errno 2]` assertion;
  another 1,000 ordinary repetitions passed.
- Process-supervision library: 17 PASS; UI-application library: 13 PASS.
  Queued ENOENT, EACCES, successful EOF, and malformed-report cases verify
  classification after helper exit and preserve its final wait status 23.
  Existing truly absent-handshake coverage still passes. The queued-before-entry
  unit cases alone also pass old code; the injected RED/GREEN establishes the
  specific accept/waitid interleaving rather than claiming those cases do so.
- Full tests for both packages, including integration targets: PASS. Strict
  all-target Clippy for both packages: PASS. Edition-2021 rustfmt and
  `git diff --check`: PASS. Builds used the shared cache, two jobs,
  incremental compilation disabled, with capacity checks before each build.

These are owned host fixtures, without device access. Exact final source cloud
CI and the complete candidate/device acceptance remain separate gates.

The UI CI job retains its existing full package tests and captures their Cargo
JSON artifact messages with pipeline failure propagation. A following gate runs
the exact UI library test executable under `strace --kill-on-exit`, delaying the
first accept4 EAGAIN return by500ms. It requires the same worker PID to next
observe CLD_EXITED through waitid and then accept the queued socket. A delay
marker alone would not prove this interleaving on a slow runner. The extracted
workflow body passed locally in0.51s with the complete required trace sequence;
a20s outer timeout bounds it and strace kills its tracees on exit. No extra Cargo
build or guard-call baseline is introduced. JSON and trace join the existing UI
CI evidence artifact. Independent final review found no Important defects.
