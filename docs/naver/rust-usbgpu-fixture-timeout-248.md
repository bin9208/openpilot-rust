# USB GPU comparison fixture deadline (#248)

The workspace check for PR #245 at `e521e6db4669877ca261c9cecee09c1a2e85302c`
failed in the ordinary `read` probe case. The source reported `GPU check timed
out`; Rust reported `12V / PCIe not ready`. Both attempted twice and reaped the
probe. The [failed job](https://github.com/bin9208/openpilot-rust/actions/runs/37824604404/job/113474075037)
records source/native elapsed times of 1.428/1.015 seconds, including the existing
one-second retry delay.

The checker used a 150 ms child deadline for every mode, including probes whose
purpose is to compare an error response. Both production implementations use a
15-second default. The checker now uses that default for ordinary modes and
retains 150 ms for the dedicated sleeping-probe timeout case. Its outer native
process deadline allows both attempts plus cleanup. Error equality, attempt
counts, cancellation and child-reaping assertions remain unchanged.

An owned preload shim held only the second source probe's actual read-error
output call for 400 ms. The old checker reproduced the CI mismatch: source
timeout versus native PCIe error. With the checker change, the same held input
produced the PCIe error on both sides, after two attempts. The sleeping probe
still timed out on both sides after one attempt at about 150 ms. All children
were reaped. This demonstrates the fixture deadline hazard; it does not identify
the runner's underlying scheduling delay.

The unchanged cancellation and missing-executable controls also pass. Focused
Ruff, syntax/deadline-selection checks and 22 CI-policy tests pass. All ten
permanent checker modes remain present.

The source, native implementation, retained Rust executables and C probe bytes
were unchanged. Existing discovery/status/power comparisons are reused; no Rust
build or hardware access was needed. Local evidence and the exact commands are
under `.omo/evidence/248-usbgpu-timeout/`, including `cause/red/result.json` and
`cause/green/result.json`. The preload shim is an untracked causal fixture, not
part of either runtime or the permanent checker.

Required CI for the resulting commit remains the integration gate. Full runtime
startup, log upload and the user's device comparison remain separate acceptance
criteria under issue #1.
