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

Four additional camera/calibration early-rejection captures are retained as
source-review evidence. They are outside the sensor regression assertions until
scope is reviewed; retaining their evidence is not a claim of native coverage.

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
