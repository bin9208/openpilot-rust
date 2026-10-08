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
| QR backup/restore codec | 173 exact payload/metadata, binary tag, decoded-object and error comparisons, including the real 208-key backup; repeated after response-order correction | `225-qr/native-order-fixed/result.json` |
| QR zlib encoder ownership | 2 focused ASan tests, including exact compression of the source 208-key golden input; standalone target about 12.2 MiB | `225-qr/asan-result.json` |
| Vehicle selection list | 21 actual HTTP comparisons, filesystem recovery/Unicode boundaries and all 341 names in the existing seven-brand assets | `carrot-server-225-resume/cars/final-wave/` |
| Cars, QR, JSON restore and backup download composition | 37 actual HTTP pairs plus 4 unavailable-Params guard cases, with persisted effects and graceful exit | `carrot-server-225-resume/http-restore-final/`, `http-restore-unavailable-final/` |
| Relocated executable root and CLI ordering | 8 cases, including 4 actual owned listeners serving relocated assets and exiting on SIGTERM | `carrot-server-225-resume/runtime-root-final/result.json` |
| Mapbox token routes and outbound validation | 85 actual source/native HTTP comparisons plus 1 encoded-path regression; response bytes, Params files and owned recipient requests match | `225-mapbox-tokens/native-fixed/result.json`, `encoded-routing-final/result.json` |
| Multipart Params restore | 44 isolated source/native pairs and the same 44 through the actual Application route; response bytes/headers, Params/history effects and 17 MiB restore match | `carrot-server-225-resume/multipart/final-standalone/`, `final-composed/` |
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

The QR implementation uses the existing Brotli encoder, safe Rust Brotli/zlib
decoders, and a narrow system `libz.so.1` encoder boundary for original zlib
compression bytes. Its codec process passed the 173-case comparison; the
37-case HTTP composition also covers QR backup and JSON restore preview/apply.
Dependency status/repair behavior remains a separate gate.
The system zlib provider is an explicit native packaging dependency.
The focused ASan run instruments the wrapper and Rust dependencies; system
zlib C and the prebuilt Rust standard library are not instrumented.

Mapbox validation preserves the original urllib eight-second socket timeout
and bounded raw response reading. Slow headers spanning about ten seconds and
body chunks spanning about eight seconds still succeed when individual reads
remain within the deadline; stalled reads fail. Unsolicited gzip/Brotli bytes
remain raw. The existing ureq 3.4.2 provider is vendored with a seven-line raw
body-reader accessor; ordinary decoding, framing and drop behavior remain
upstream. Its provenance and license are retained in
`rust/vendor/ureq/PATCH_NOTES.md`. Focused tests cover ordinary decoded access,
raw access and dropping a partial body before another request. All outbound
comparison recipients and token values are owned fixtures, with no live Mapbox
validation request.

Multipart restoration uses maintained multer 3.1.0 for framing and field reads.
The bounded vendor changes retain duplicate headers in order, remove its
32-header storage limit, and enforce the original 8,190-byte name/value limit
before header normalization. Original trailing whitespace and header spelling
remain available for the observed errors. A build-script declaration names the
upstream `nightly` cfg without suppressing diagnostics. Upstream revision,
license and patch scope are retained in `rust/vendor/multer/PATCH_NOTES.md`.
The source's first-part behavior, raw content encodings, extended field names,
initial `_charset_` failure behavior and closing-boundary errors are covered.
The 17 MiB case deliberately uses multipart field reads rather than the JSON
request-reader limit, matching the original. Exact non-UTF-8 codec aliases and
some decode-error text remain part of the shared request-codec work below;
these 44 cases do not establish universal Python codec equivalence.

## Remaining work

Profiles, restoration and change-history services have independent process
evidence, and profile/history HTTP routes and real index bootstrap are connected.
Multipart restoration has isolated and composed HTTP evidence; QR dependency
status/repair remains in progress.
The request decoder currently handles UTF-8, Latin-1 and ASCII; other original
request charsets and compressed request bodies remain transport gaps. Multipart
extended names also use the existing encoding_rs provider for supported labels,
with broader original codec aliases/error behavior still explicit gaps.
The other feature families, startup heartbeat/git/update/upload tasks, live
broker and camera/WebSocket transport remain outside the completed foundation.
`/stream` also depends on the separately inventoried WebRTC conversion.
The executable now resolves runtime assets using existing OPENPILOT_ROOT/BASEDIR
environment conventions or executable/working-directory ancestors, without a
build-checkout fallback. Eight relocated CLI cases pass; this exercises the
partial listener, not the unfinished whole-app startup/background services.

Complete original server behavior, required branch CI, the small dev connection
check and full manager startup with the existing log-upload path remain gates.
The user performs the first device comparison only after the complete runtime
candidate is ready. These host observations establish no CPU savings or vehicle
acceptance.
