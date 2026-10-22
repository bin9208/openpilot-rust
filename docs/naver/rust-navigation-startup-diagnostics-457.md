# Navigation pre-child startup barrier diagnostics (#457)

Full917cd7db run39567841179/job119114353901 reached the unchanged10s
cancellation-start barrier with the controller alive and no output directory,
probe marker, source record, stdout or stderr. No phase trace existed in that
run. That evidence does not establish RSA generation, an output lock or another
specific root cause.

Opt-in source7d16f776 diagnostics add seven bounded phase markers and retain
/proc state on an actual barrier failure before owned cleanup. Full navigation
39571317568 passed; this validates instrumentation and existing checks, not a
repair or reproduction of the intermittent startup stall.

The bounded follow-up [39617213517](https://github.com/bin9208/openpilot-rust/actions/runs/39617213517)
passed source1f7a182b0b9708340c5f16b5ed55d6c470f5ad8b/workflow
84d22b4780729cda7362b18ae9952815cc8ac289: twelve fresh cancellation-test
processes, all original assertions and10s barriers, then the complete navigation
source/HTTP/authentication/IPC/lifecycle regression and strict Clippy. Parent
audited artifact12354710648 (37,344,767bytes/1,650members, all CRCs), ZIP SHA256
`eb23b5a14a51fd413678d34c17e3b205eee276c8d20b418fb599b29f11335526`.
All12 retain state/probe/phase evidence, signal2 and reaped probe, and no false
success report. They did not reproduce the stall. Keep457 open and retain
failure diagnostics in the maintained full gate; no root-cause or device claim.

The later full navigation run
[39719156536](https://github.com/bin9208/openpilot-rust/actions/runs/39719156536)
failed the same 10-second barrier. Its controller was runnable for 10,003 ms,
with 946 user CPU ticks and no source record or probe marker. That build had
only the earlier phase markers, so the failing operation remains unproved.
Source `12fab44083b6dbd1f8c69f5f2b18bea8aca99461` added opt-in markers
around source, worker, binary and candidate hashing and fresh RSA generation.
The pinned follow-up
[39721672474](https://github.com/bin9208/openpilot-rust/actions/runs/39721672474)
passed all eight fresh cancellation starts and the full navigation suite.
Artifact `12401668808` has 1,682 CRC-valid members and SHA-256
`16710899eb402c380c51afefe95598b2b3a4b36741b73a4c80a038c7834bc559`.
RSA generation took 2.04–7.77 seconds across the eight attempts; candidate
hashing took about 2.6 ms. This identifies the dominant measured admission
cost in successful runs, not the exact phase of the historical failure.

The next scoped candidate compiles the locked `num-bigint-dig` dependency at
`opt-level=3` for Cargo's `dev` profile; `test` inherits that profile. It leaves
fresh random 2048-bit RSA generation, its error order, release builds and the
10-second start barrier unchanged. The same pinned eight-attempt/full-suite
hosted gate must measure the candidate before attributing any timing change.
Even a faster passing run would not establish that the intermittent failure is
fixed without a reproduced failure or otherwise discriminating evidence.
