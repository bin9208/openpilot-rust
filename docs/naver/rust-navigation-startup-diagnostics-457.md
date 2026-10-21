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
