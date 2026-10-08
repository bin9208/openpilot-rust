# Carrot Web runtime conversion: issue 225

This is an incomplete implementation stage of the full runtime conversion in
[issue 1](https://github.com/bin9208/openpilot-rust/issues/1), tracked in
[issue 225](https://github.com/bin9208/openpilot-rust/issues/225). The Rust server
is not registered as an available complete runtime. Production daemon selection
is unchanged, and no vehicle, NAS or real upload recipient has been contacted.

## Source boundary

The original entrypoint is `openpilot/selfdrive/carrot/carrot_server.py`.
Reachable application, feature, service and realtime modules contain the actual
runtime. Existing frontend assets remain source inputs. The port reuses Rust
Params, logging JSON values and upload URL policy rather than changing the
original settings or adding options.

The first implementation covers configuration/state migration, settings/menu
loading, typed Params reads, unit indices, favorites and web-settings policy.
Original Unicode, JSON scalar conversion and partial temporary-file behavior
are observable contracts. Registered malformed numeric Params can terminate
the original Cython process with SIGABRT; the corresponding native process
tests preserve that termination rather than silently returning a fallback.

## Intermediate evidence, 2026-10-08

Evidence is local under `.omo/evidence/`; these are intermediate host checks,
not complete-server acceptance.

| Boundary | Observed result | Local evidence |
| --- | --- | --- |
| Initial settings/filesystem policy | 5 filesystem tests, 41 source policy cases | `carrot-server-225-resume/baseline.md` |
| Actual HTTP settings, Params, profiles, history and index composition | 103 source/native request comparisons, real bootstrap, destination/temporary bytes, actual Git metadata and graceful exit | `carrot-server-225-resume/http-provider-bootstrap-first/` |
| Supported typed Params/TIME and backup reads | 66 source/native cases, including 13 full 208-key backup objects | `carrot-server-225-resume/params-backup-final/` |
| Original malformed numeric Params termination | 6 source/native SIGABRT cases with core dumps disabled | `carrot-server-225-resume/params-fatal/` |
| Web-settings normalization/catalog/capabilities/persistence | 702 source/native cases plus 2 focused Unicode/legacy-host regressions | `225-web-settings/README.md` |
| Profiles, restoration and change history | 73 service-process comparisons with real Params/file effects, drift representation, 1,000-record trimming and Git output/timeout boundaries | `225-profiles/native-fixture-fixed/result.json` |
| Intro guard, state and presets | 70 source/native HTTP/file comparisons plus 2 cases without Params; actual preset writes and original failure contracts | `carrot-server-225-resume/intro/final-wave/result.json` |
| Isolated static HTTP | 61 source/native request pairs, 6 compressed files and graceful exit after lazy bootstrap callback integration; conditional obs-text regressions repaired | `carrot-server-225-resume/static/lazy-wave/` |
| Optional Brotli absent | Original and native remove stale `.br` while producing valid gzip | `carrot-server-225-resume/static/absent-codec-comparison.json` |
| Brotli Rust wrapper ownership | 2 focused tests pass under ASan in a 2,992 KiB standalone target | `carrot-server-225-resume/static/LEDGER.md` |

After replacing manual clamp syntax with an explicit NaN fallback, the existing
19 ratio boundary/error cases passed again in `225-web-settings/ratio-clamp-final/`.
This preserves the original NaN result without a Clippy suppression.

Executable identities, individual comparison inputs and commands remain in
those receipts. Different waves retain their actual configurations. The first
foundation was committed as `6900a254`; later table entries also include the
subsequent implementation wave and must not be attributed to that commit.
Backup objects match by exact keys and typed values. Their raw JSON key order
differs because the original enumerates a C++ unordered map while Rust uses
sorted metadata; raw lines retain that distinction rather than claiming byte
identity for noncanonical JSON serialization.

## Static HTTP compatibility in progress

The static implementation preserves conditional requests, ranges, directory
containment, compressed variants, index bootstrap and asset cache policy.
Full production routing and lifecycle composition are still in progress.
The isolated fixture supplies the original bootstrap payload; it does not
establish live web-settings/intro bootstrap composition. A shared `StaticWeb`
instance is now connected to the application router and startup precompression.
The subsequent 73 core HTTP comparisons verify that an unavailable index returns
503 without creating Intro state. Bootstrap runs only after index loading and
recovery retries succeed, matching the original order. The 103-case composition
run also verifies successful index delivery using actual Params languages,
language assets, web settings/capabilities and Intro state. Profile creation
uses real UUID/UTC and four commands in an owned Git repository; rejected
empty/malformed/full-store creates invoke none. Profile JSON is reserialized
after normalizing captured UUID/time fields; raw responses and state are retained.
Explicit Latin-1 request text, including the C1 byte mapping to U+0080, is covered.

The Intro and direct setting-write probes also preserve the original Cython
adapter's registered-write contract: it validates keys and conversions but
discards native persistence return status. A directory blocking one registered
Params file can therefore leave partial writes while the source reports success.
The server-local adapter matches that observed behavior; unknown-key, conversion
and unregistered Python-style atomic-write errors remain distinct. The shared
Rust Params API is unchanged. Failed and corrected receipts are retained.

Hyper 1.11.1 normalizes trailing optional whitespace that aiohttp 3.13.3 retains
for its Range and conditional parsers. A vendored, default-off server option
captures only those six original values from the already-framed request head.
Carrot Web opts in. The normal header map and message/body framing remain the
upstream implementation. License, upstream revision and patch scope are kept
in `rust/vendor/hyper/PATCH_NOTES.md`. Actual default/off/on, duplicate-header,
body-framing and persistent-connection checks accompany source comparisons.

Optional Brotli encoding uses versioned system `libbrotlienc.so.1` and
`libbrotlidec.so.1`, with retained library handles and bounded owned buffers.
Their transitive system dependencies and deployment availability must be
declared in packaging. Codec-present and codec-absent behavior are distinct
checks; absence must not be presented as proof of compression. Native library
execution does not imply that distro C code was sanitizer-instrumented.
The bounded ASan run instruments the Rust wrapper and its Rust dependencies;
the distro C libraries and prebuilt Rust standard library were not instrumented.
The full static handoff is recorded in `carrot-server-225-resume/static/LEDGER.md`.

## Remaining work

Profiles, restoration and change-history services have independent process
evidence, and profile/history HTTP routes and real index bootstrap are connected.
QR codecs and complete backup/restore HTTP flows remain in progress.
The request decoder currently handles UTF-8, Latin-1 and ASCII; other original
request charsets and compressed request bodies remain transport gaps.
The other feature families, startup heartbeat/git/update/upload tasks, live
broker and camera/WebSocket transport remain outside the completed foundation.
`/stream` also depends on the separately inventoried WebRTC conversion.
The executable still derives its repository path from the build checkout;
runtime asset-root discovery and a relocated-executable startup check remain
entrypoint gates before deployment readiness.

Complete original server behavior, required branch CI, the small dev connection
check and full manager startup with the existing log-upload path remain gates.
The user performs the first device comparison only after the complete runtime
candidate is ready. These host observations establish no CPU savings or vehicle
acceptance.
