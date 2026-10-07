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
