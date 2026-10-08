# CarrotMan runtime candidate (#219)

This candidate ports the original CarrotMan owner, CarrotServ decisions,
navigation ingress and source selection, route handling, diagnostics and upload
side effects. It is part of issue #1. Production process selection is unchanged.

`openpilot-carrot-man` calls the same native actor as the owned-process example.
The latter supplies private Params paths, loopback ports and owned HTTP/upload
recipients. It does not replace the native IPC, socket or serialization paths.
The existing Navd JSON/numeric helpers and web-upload transport are reused.

At `31ae4fac82551e7280874379f5aec3e1b0f3e5ab`, both native owner jobs in
[run 37652025478](https://github.com/bin9208/openpilot-rust/actions/runs/37652025478)
passed the complete original-source/process comparison:

| Architecture | Exact job | Retained artifact |
| --- | --- | --- |
| x86_64 | [112898227575](https://github.com/bin9208/openpilot-rust/actions/runs/37652025478/job/112898227575) | [carrot-man-owner-x86_64-evidence](https://github.com/bin9208/openpilot-rust/actions/runs/37652025478/artifacts/11498050138) |
| aarch64 | [112898227346](https://github.com/bin9208/openpilot-rust/actions/runs/37652025478/job/112898227346) | [carrot-man-owner-aarch64-evidence](https://github.com/bin9208/openpilot-rust/actions/runs/37652025478/artifacts/11496762389) |

Both `owned-summary.json` files report `passed=true`, `compared=true` for 12
boundaries: real Params/IPC, TCP session/terminal, HTTP auxiliary ownership,
duplicate-client close, route framing/partial rejection, KISA UDP, ZMQ echo,
manual upload to both targets and Discord, onroad network wait, CAN offroad
cancellation, exception clearing/sent flag, and late UTF-8 expiry/clear/recovery.
The final comparison includes all selected Cereal fields, owned Params side
effects and complete observed upload fields/attachments. Only the existing
owner/safety ages, traffic/image/event receipt times and upload filename/time
normalization are excluded.
Native identity artifacts show the built `carrot_man_owned` executable with
`libpython=false`; the source process uses the original Python/Params/msgq path.

The host job also passed 32 policy/geometry/source-arbitration cases, 24 ordered
CarrotServ publication/state cases, 19 strict/legacy ingress cases and six
time-setting command-boundary cases. Those helper checks include active GEOS and
the optional missing-dependency route path. The ARM job runs the full owned
process comparison; its helper-policy step is intentionally skipped.

The x86_64/aarch64 owner matrix remains a dependency of the required `rust checks`
aggregate. Common workspace gates were still pending when this evidence was
recorded, so these module results do not establish that the entire PR is green.
Local Cargo builds remain paused under the repository's disk reserve.

Native dependencies include Cereal/msgq, ZeroMQ and optional GEOS. The GEOS
provider is pinned to the Shapely 2.1.2 wheel's GEOS 3.13.1 libraries; archive,
library hashes and architecture are checked before loading. Normal deployment
with the original Shapely-present behavior must package that provider and set
`CARROT_GEOS_LIBRARY` and `CARROT_GEOS_MANIFEST`. Missing optional GEOS retains
the original dependency-absent path. ZeroMQ, Linux APIs and the original
tmux/shell/time-setting command dependencies remain explicit.

Complete manager packaging, normal startup/upload composition and the user's
first device comparison remain open. The manager catalog exposes this isolated
candidate while retaining the original process registration and production
selection. The independent Carrot Navi receiver (#206), Carrot Server, vehicle
startup and complete runtime composition have separate integration gates.
All upload/notification recipients were owned loopback fixtures. No real
recipient, NAS, vehicle or host time/zone mutation was used.

## Owned fixture port allocation (#232)

The dev post-merge [ARM job 113237686782](https://github.com/bin9208/openpilot-rust/actions/runs/37755126660/job/113237686782)
at `edd1ab836c6536c2c77248173e11a90d390b9ff0` failed while waiting for
the binary-route publication. Its retained native stderr reports
`route listener: Address already in use (os error 98)` followed by navigation
`unterminated_frame`. That run did not record its allocated ports, so duplicate
route/navigation ports remain an explanation, alongside an unrelated port owner.
[Issue #232](https://github.com/bin9208/openpilot-rust/issues/232) tracks the
fixture correction; the later unchanged CarrotMan ARM success at `7a8d6419`
does not establish the cause of the earlier failure.

A deterministic recurring-port fixture executing the old allocator produced
seven copies of port 32797. Real navigation and route TCP binds then reproduced
errno 98. The allocator now holds all seven TCP reservations through selection
and fixture preparation, asserts that the set has seven distinct values, and
releases it immediately before child launch. Each implementation retains its
complete `fixture-configuration.json`, including the role-to-port map.
No runtime code, retries or delays changed. External port claims after release
and before the child binds remain possible; numeric TCP reservations also do
not reserve UDP listeners in another process.

Two focused real-socket tests passed: recurring candidates remain distinct and
unavailable to another bind while reserved, then become bindable on release;
preparation failure also releases every reservation. The existing actual
source/native owned comparison passed all twelve boundaries once using host
ELF SHA256 `34da06d71dc846666ba3cc1f733b0262a511fc5818609055d8cdf8a7cd60b9b1`,
the cached original Params/msgq bindings, and verified Shapely 2.1.2 GEOS
libraries. Both processes captured seven distinct ports, the expected
two-point route publication and destination, and the existing upload and
recovery side effects. Native process identity recorded `libpython=false`.

Private local evidence is under `.omo/evidence/carrot-man-232/` in the issue
worktree: `released-port-reproduction.json`, `allocator-{red,green}-invocation.json`,
`owned-comparison-invocation.json`, `owned-comparison/owned-summary.json`,
and both implementations' configuration/publication/selected-result files.
The coordinated primary build reused unchanged CarrotMan/msgq/Cereal/web-upload
sources; additive messaging State and Params APIs from concurrent work are
listed in `source-dependency-readback.json`. No local ARM rerun or new exact-head
Actions result is claimed here; those remain the parent's PR validation gates.
Both CarrotMan CI architectures now run the two standard-library socket tests
before the existing owned comparison, using the bindings already built by that job.

PR #234 at `b0a5234eb11fc9937c6bfa64e55c656824f53041` passed both
CarrotMan architectures, but the general [workspace job](https://github.com/bin9208/openpilot-rust/actions/runs/37764599286/job/113269028435)
failed while collecting the new test: importing the complete owned-comparison
driver required `msgq.ipc_pyx`, which that workspace step does not build.
The unchanged reservation function now lives in the standard-library-only
`carrot_man_fixture_ports.py`, imported by both the driver and its tests.
An isolated Python invocation failed before this extraction and passes both
socket tests afterwards; AST comparison confirms the function is unchanged.
The same two tests pass under pytest, and all eighteen isolation-policy tests
pass. No runtime build or twelve-boundary comparison was repeated for this
import-only repair. Updated exact-head Actions remain required.
