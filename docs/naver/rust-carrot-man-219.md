# CarrotMan runtime candidate (#219)

This candidate ports the original CarrotMan owner, CarrotServ decisions,
navigation ingress and source selection, route handling, diagnostics and upload
side effects. It is part of issue #1. Production process selection is unchanged.

`openpilot-carrot-man` calls the same native actor as the owned-process example.
The latter supplies private Params paths, loopback ports and owned HTTP/upload
recipients. It does not replace the native IPC, socket or serialization paths.
The existing Navd JSON/numeric helpers and web-upload transport are reused.

Earlier independent helper comparisons passed 32 policy cases, 24 consecutive
CarrotServ message/state cases and 19 ingress comparisons. A source-only process
preflight exercised the original Params, IPC, TCP/HTTP/UDP, route, diagnostic and
upload paths. It did not compare a newly built Rust owner. The final surrogate
and recovery scenario and the complete native owner still require execution.

The CI candidate builds the real owner and examples on x86_64 and aarch64,
compares the original source using owned inputs/recipients, and retains results.
It feeds the existing required Rust aggregate. Local native compilation is
blocked by the repository's disk reserve; source implementation is not a claim
of completed independent runtime validation.

Native dependencies include Cereal/msgq, ZeroMQ and optional GEOS. The GEOS
provider is pinned to the Shapely 2.1.2 wheel's GEOS 3.13.1 libraries; archive,
library hashes and architecture are checked before loading. Normal deployment
with the original Shapely-present behavior must package that provider and set
`CARROT_GEOS_LIBRARY` and the verified manifest configuration. Missing optional
GEOS retains the original dependency-absent path, not full route-preview evidence.

Complete manager packaging, normal startup/upload composition and the user's
first device comparison remain open. No real recipient, NAS or vehicle was used.
