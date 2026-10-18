Issue #373 independent original admission capture.

`original.json` SHA-256:
24d21ae26bf1af7135e8e60a2372a98fb78cb5620c58d64df2f932bae258d188

Original `openpilot/selfdrive/locationd/locationd.py` SHA-256:
aa29e3a4da21007bd93a99713f87c07b79407a52e067a1fdf82019c967103f5e

Captured before any production repair using CPython 3.12.15 and NumPy 2.5.3.
The capture compiles unchanged original class methods and the original main
`if filter_initialized` branch. Its mechanical filter boundary records calls and
returns no observation. All 16 stale/BMX055 rejection controls and all 40 loop
controls make ZERO numerical observation calls. Two genuine accepted controls
check only admission/result and record one observation call; no numerical filter
state equivalence is claimed from this boundary fixture. Six genuine malformed
measurements preserve the original IndexError rather than invent missing axes.

The first 16 direct controls cover accelerometer/gyroscope, vector lengths 0..3,
and stale timestamp/BMX055. The 40 loop controls cover the same 16 initialized
cases and all 24 uninitialized cases. Uninitialized inputs are not interpreted
when the original main loop has not admitted its initial subscriptions.

Four additional camera/calibration early-rejection captures are retained and
covered by the later camera/calibration regression assertions described below.

Capture source: ignored
`.analysis/scratch/2026-10-18-locationd-tools/capture-admission-373.py`.
No runtime candidate produced an expected value. The initial native regression
ran against the unchanged wire admission and failed two of three tests. After
repair, all five sensor admission tests pass in a fresh current-source build.
Source fixture identity is retained independently of that build.

`followup-original.json` SHA-256:
5e65ed71110ebfae7ebd12888d3a935649e77f7986dc0e4e2013181f9989f6b0

Additional unchanged-source branch captures cover six first-cycle initialization
cases with all subscribed services present: missing axes are not interpreted, but
sensor validity still determines initialization. Two five-step sequences preserve
the stale-input increment, ignored-source no-change and genuine valid-input decay.
The same source main branch and record-only observation boundary are used; no
numerical state or covariance expectation is fabricated. Captured original invalid
counters retain the existing 1e-12 comparison budget. First three native tests
observed a real current-source RED before the production repair; these additional
controls extend initialization/recovery coverage and pass. The complete locationd
package passes 17 tests and strict all-target Clippy. A private real Msgq probe
using the freshly built daemon (SHA-256
806856789b4c4fc34f8d6f04a1ccd4d2096e3b02f91ddd87d3e3ade4b660febd)
published three livePose messages and exited successfully after uninitialized
malformed, initialized stale/secondary, then genuine valid sensor inputs. Its
daemon PATH contained no executables. This establishes host continuation, not
full independent EKF numerical equivalence or physical-device acceptance.

The camera/calibration follow-up preserves timestamp, geometry, minimum and norm
admission order before malformed dimensions become errors. Calibration bounds
still inspect all entries and propagate NaNs; late camera standard-deviation
shape failures retain the original posenet history mutation. Ordinary three-axis
inputs keep the fixed-size numerical path and no missing axes are synthesized.

`camera-calibration-original.json` contains 34 unchanged-original captures
(24 returned results and 10 ValueErrors), SHA-256:
1fb7962961354bf1e47c0d5d849ed97ccf7c06c4ecde1759345c43e7ae192e22

`camera-calibration-edges-original.json` contains 29 additional captures including
NaNs, short-circuit ordering, bounds and nine late history mutations, SHA-256:
a12994a7de598f5ec1637dd90a2767c51f24bcf14596279d8e9cd0998a05c4b9

The first four native regression tests all failed before this repair. The final
complete package passes 23 tests and strict all-target Clippy, including exact
history checks and unchanged Kalman state for all 29 non-observation cases.
A freshly built daemon (SHA-256
bab6660704ecfbc5bb17e9719b13dc413cade0d258abc3bcc934adc8542bdf7b)
then published seven livePose messages through a private real Msgq namespace,
covering malformed/rejected sensor, camera and calibration inputs followed by
valid inputs, with no executable on its PATH. It exited successfully. Private
records are `2026-10-18-locationd-tools/camera-calibration-green1-*` and
`camera-calibration-live1/proof.json`. These establish host admission and
continuation, not full independent EKF numerical parity or device acceptance.
