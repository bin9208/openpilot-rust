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
On 2026-10-09 the user authorized deleting old records and reproducible build
outputs to prioritize the complete candidate. Some historical local paths below
were removed; their recorded outcomes remain historical summaries, not claims
that those files still exist. Current unfinished-family evidence is retained.

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
| SSH key routes and recipient boundary | 28 cases each with native Params, memory fallback and absent client; 3 further response-codec/total-timeout cases | `carrot-server-225-resume/ssh/LEDGER.md` |
| SSH production timestamp conversion | Original float-seconds expression and native production helper agree at the retained rounding boundary after an observed failing test | `carrot-server-225-resume/ssh/clock/result.json` |
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

SSH requests use the source ten-second total deadline and decoded response
bodies, independently of Mapbox's socket timeout/raw-response contract. The
current original Params catalog does not register `GithubSshKeysUpdatedAt`:
add/remove requests can therefore fail after changing earlier known keys.
Actual owned Params comparisons preserve those partial writes and errors.
No real GitHub key recipient or sshd was used. Provider User-Agent identity,
untested TLS/redirect/cookie variants and the complete response-codec namespace
remain explicit limits in the ledger.
The 87 HTTP observations use recorded fixed timestamps. A separate focused
test covers the production `int(time.time() * 1000)` rounding behavior; these
different configurations and executable identities are retained separately.

## Request transport and Web Sound checkpoint (2026-10-08)

The shared request reader now owns one content decoder and prefetched frame
queue across JSON and multipart consumers. Supported compressed bodies and
additional charset/error cases have actual source comparisons. Parser errors
observed before dispatch take precedence over feature availability; payload
errors remain available to the feature's first read. A per-request context
retains connection-close intent even when a feature catches the payload error.

The retained application ELF is
`7b22124f929f4980a8f328ea00959c1b329d3f9dffb344c23ea0cda4cc14ee5c`.
Its focused comparison passes 44 HTTP pairs, checking status/version, headers
except Date/Server, raw body and stored effects. Complete response bytes and
EOF match for the tested parser-error connections and a caught-payload 200
response. Healthy chunked data/trailers followed by a second request also match.
The build, strict all-target Clippy, formatting and three WebSocket library
boundary tests pass. These results and rejected probes are retained under
`.omo/evidence/carrot-server-225-resume/request-decoding/transport-fixed*`
and `transport-sound-fixed/`.

The same ELF also matches six actual WebSocket protocol observations, including
custom and empty peer Close, compression refusal and message-size boundaries.
With an invalid actual Params root, both implementations complete the 101
upgrade, publish no state, answer Ping and exit cleanly after peer teardown.
These comparisons are in `.omo/evidence/225-web-sound/native-protocol-fixed/`
and `native-params-init-failure-fixed/`. Stateful live IPC and full heartbeat
comparisons were separate pending checks at this checkpoint.

The Hyper fork adds opt-in response-close handling, a server receive-buffer
size option, and inspection of already-framed Incoming data without requesting
another read. Close intent is captured before header serialization, then applied
only after successful encoding. The ordinary provider behavior stays default-off;
an actual default-off TCP control remains outstanding at this checkpoint.
The tungstenite 0.29.0 fork adds an opt-in server reply of code 1000 with an empty
reason to a valid peer Close. Tests cover default/client behavior, invalid codes,
and ordering after a partially written application frame. Provenance and licenses
are retained in both vendor directories. The shared Carrot Navi handshake helper
was extracted without changing its function body; its native-feature test passes.

Three observed transport gaps were open at this checkpoint: a fully buffered chunked deflate EOF
can still reach an unavailable feature before its parser error; Expect handling
omits the source's early 100 response and unknown-value 417; and active Web Sound
sessions currently exit immediately on server shutdown. The original retains
the active connection while discarding newly received data until peer EOF or
its existing cleanup deadline. A separate partial-body probe returns the same
immediate 500 response and effects but closes at about 0.5 ms instead of the
source's 10.23 s. This cleanup timing difference is retained explicitly and does
not authorize adding another transport timer.

### Framing, Expect and active-session follow-up

The subsequent retained application is
`24f855658c266e29299d64336828fc3b295ca82a5d3bf7bbd22349eeceadcc47`.
It passes the same 44 strict HTTP comparisons and all 11 captured Expect cases.
Already-buffered body bytes are framed by the existing Hyper decoder before
dispatch, without another socket read or a separate parser. Full chunked deflate
EOF now reports the original parser error before an unavailable-feature guard;
a terminal chunk arriving only after the guard response does not change it.
Healthy chunk/trailer framing and a second request on the same connection pass.

A typed cause retains the raw line only for the existing buffered first-digit
chunk-size rejection. The captured `zz` case now returns the source's exact
47-byte HTTP/1.0 400 response. An earlier payload decoder failure still takes
precedence over this later framing error, as the original does. This narrow
diagnostic adaptation does not establish equivalence for every provider error.

Application-controlled Continue uses Hyper's existing notification and header
buffer paths. It follows parser inspection, supports empty bodies and pending
body reads, survives request destruction, and emits at most one 100 response.
Unknown expectations return 417 before feature effects. Empty Expect is ignored;
trailing whitespace is retained through the existing raw-header collector, now
including Expect alongside the six original conditional names. Four actual TCP
controls verify default-off behavior, unmarked keepalive, a complete marked
1 MiB response followed by EOF, and repeated Continue requests. The focused
six-header/default-off/static framing smoke also passes.

Web Sound's unchanged production runtime is independently verified on retained
application `a61be06b33f52de40fccfcf6d9058ab6be773b49a610f8d56d7e75868dedb316`:
six protocol observations, 14 exact live IPC state texts, three real heartbeat
cases, unavailable-Params and sender-failure boundaries, and two active shutdown
comparisons pass. During graceful shutdown new inbound bytes are discarded,
while state output and heartbeat continue until peer EOF or the existing server
cleanup deadline. A separate real IPC/TCP test verifies explicit forced cleanup
and sender cancellation. The current test executable repeats that assertion in
an isolated child when no owned namespace is supplied; default and unowned
environment invocations both pass without mutating the test process environment.

Strict package Clippy, formatting, client-only Hyper compilation, four TCP
transport tests and four Sound boundary tests pass. Commands and retained
identities are under `framing-expect-edges/`, request-decoding `framing-final*`
and `expect-*fully-fixed/`, and `.omo/evidence/225-web-sound/`.
The partial unread-body EOF timing difference remains explicitly accepted as an
external-provider cleanup limit; its raw comparison still reports that difference.

## eGPU model and Xiaoge HTTP adapters (2026-10-08)

The eGPU model status/restart routes reuse the existing native model manifest,
installed-artifact and local-compiled-path helpers. A read-only parser preserves
the original stored-state rules. Actual owned model, USB-sysfs and Params files
cover status, guards, disappearance during a read, atomic status persistence,
partial write failures and recovery. Independent and Application executions each
match 79 source HTTP/effect cases and two unavailable-Params cases. The adapter
writes the existing `DoReboot` flag after status persistence; the fixture does
not reboot, download a model or access hardware. Commands and exact binaries are
recorded in `.omo/evidence/carrot-server-225-resume/egpu/LEDGER.md`.

The Xiaoge adapter covers the existing page and fixed-localhost diagnostic proxy,
separately from the diagnostic/inference backend. The retained standalone ELF
`881a7241c78203a9048ed4b92416f5c2beec5f96f9ac792923a2af5a35ef2cdb`
passes 98 HTTP comparisons, one additional delayed empty-response deadline case,
one refused-dependency case, and nine repeated deflate-boundary observations.
Ten cases also pass through the Application router/listener. Comparisons retain
status, selected headers, raw response bytes and actual outbound requests.
They cover origin/method guards, request and response limits, compression,
redirect refusal, request-stage retry, dependency recovery and process shutdown.

The ureq response extensions retain removed Content-Encoding and expose whether
a length-delimited body was already buffered when headers completed. They do not
add I/O or predict another provider's feed partitioning. Xiaoge uses this limited
observation for the captured truncated-deflate parser/consumer error order.
Other partitions and saturated-connect timing remain unverified; response timing
is not claimed identical. Four actual TCP provider controls and strict package
Clippy pass. The independent and composed receipts, rejected earlier candidates
and source identities are in `.omo/evidence/225-xiaoge/completion.json`.

Popular-values uses the existing reqwest 0.13.5 async provider with a persistent
client and cookie jar. The original startup/cleanup callbacks cancel an owned
stalled POST in about 0.2 ms; the corrected native process exits in about 1.1 ms.
The earlier synchronous candidate waited about 1.06 s for its configured timeout
and remains preserved as the failing comparison. Async task cancellation now
also cancels the in-flight request.

The corrected retained example ELF is
`b0dd8b60c484c7fbd38888483744169f8c812ece2ea7d8d2a84419e5e4742813`.
It passes 58 source HTTP pairs, nine Application observations with three clean
exits, eight focused cookie pairs, and the rounded-deadline control. The unchanged
109 policy cases are reused with checked source identities. Upload retries,
source redirect method/header rules, cookie persistence with IP-cookie rejection,
cache refresh and failure recovery use owned loopback recipients. No real
settings or service credentials are sent. The single total deadline retains
the source's monotonic rounding when the configured timeout is at least five
seconds. Existing dependency versions stay pinned; the cookie feature adds only
publicsuffix 2.3.0 and psl-types 2.0.11 to the lockfile.

Receipts are under the `popular-values` directory in
`.omo/evidence/carrot-server-225-resume/`, including the original cancellation
capture and `async-http`, `async-composed`, `async-cookies`, and `async-rounding`.
Cookie comparisons verify parsed name/value pairs, persistence and rejection;
the maintained native jar serializes multiple distinct cookies in a different
order from aiohttp. Both raw headers are retained, and raw Cookie byte identity
is not claimed. TLS/real-service and target-device conditions remain unverified.

## Repository lock transfer prerequisite (2026-10-08)

The existing process-child launcher can now transfer one repository lock into
an owned session. Git status and update commands need the original lock's open
file description to remain held when their parent closes its descriptor. The
opt-in path passes that descriptor over the existing private Unix socket with
SCM_RIGHTS, requires a child acknowledgement, and retains it across exec.
The parent's descriptor flags are unchanged. Missing acknowledgements fail
closed and clean up the owned process group; the existing zero-descriptor
launch path remains the default.

Three real-kernel unit tests and three child-process tests pass: lock ownership
survives parent close until child exit, malformed/multiple/truncated transfers
release their descriptors, exec failure releases the lock, and an older helper
that ignores the new field cannot leave its descendant running. The existing
checkout-status and updated command examples also pass with helper SHA256
`9b1a94bed0fd3593fca823aba8d906884408cb37bec55826d6d23a207c2e6840`,
retaining separate stdout/stderr and merged output/nonzero exit behavior.
Receipts are in `.omo/evidence/225-git-status/direct-consumers/` and
`.omo/evidence/carrot-server-225-resume/bluetooth/coordinated-checks/`.
Strict all-target Clippy for the shared package and both adapter packages, and
workspace formatting, pass in `bluetooth/final-checks-retry/`.
This is a shared launch prerequisite; Git status and automatic-update service
conversion and their complete source comparisons remain in progress.

## Git status service prerequisite (2026-10-08)

The Git status service now runs the original tracking-resolution, fetch and
ahead/behind commands through the shared native child launcher. It retains
the 600-second cache, non-forced request coalescing, separately serialized forced
refreshes, nonblocking repository lock and original eight-/twenty-five-second
command limits. The periodic loop retains its eight-second initial delay and
sixty-second interval.

Twenty-nine bounded source/native scenarios pass against actual owned Git
repositories and local bare remotes. They cover tracking variants, missing or
diverged refs, cache refresh, contention, forced/non-forced callers, recovery,
timeouts and cancellation. The final service example SHA256 is
`0f26233099837deb64ad9743428ef2ee65efb704c2b6748a9f3d7b43730aaa4a`;
the matching retained helper is `9b1a94be` above. Exact commands, raw outputs,
per-case binaries, thirteen source identities and reused-proof boundaries are
in `.omo/evidence/225-git-status/completion.json`.

Two observed cleanup differences were repaired before freezing. A process wait
started while the original leader is live also waits for its output pipes;
Rust now retains that phase within the existing one-second TERM grace before
KILL and reap. The live-command and fetch examples consequently finish near
nine and twenty-six seconds on both sides, while an already-observed exited
leader keeps the immediate-cleanup path. Cancelling a queued periodic request
also now waits only for that request's own cleanup, so an unrelated API refresh
does not delay cancellation. Four cleanup rows and two lifecycle rows were
repeated after these fixes; twenty-three unaffected scenarios were reused.
The failing candidates and actual PID-exit receipts remain retained.

Strict all-target Clippy and formatting pass. The immediate-cancel fixture
records no Git launch on either side and does not establish cancellation during
active descriptor transfer. A matching valid launcher is required; deliberately
stalled helpers, device timing and precise scheduler equivalence are unverified.
The real HTTP status route and Application polling/cleanup connection await
the original Git state persistence primitive. This service checkpoint does not
complete the automatic updater or normal server startup.

## Bluetooth and screenrecord HTTP adapters (2026-10-08)

Bluetooth setup now reuses the native BlueZ coordinator, configuration parser
and atomic writes. A snapshot reader shares the existing connection so status
requests can complete during a pending device action or radio command. The
original same-origin, JSON, 32 KiB body and fresh stationary-state guards remain
in the adapter, including the second guard after acquiring the mutation lock.
Lazy body-read failures retain their original unhandled-500 phase and
`Connection: close`; JSON and mutation failures retain their separate mapping.

The retained Bluetooth example SHA256 is
`80b950ddf775ae29537ed656915df0b0b5b45bdca772d67799658084cf6a49a5`.
It passes 85 independent HTTP pairs, eleven Application pairs and five owned
timeout/concurrency pairs, including pending-pair cleanup, status during a held
radio command, the original three- and twenty-second deadlines, reaping and
recovery. Old snapshot readers reject requests after close; the existing
yielding BlueZ command consumer passes its four-command ordered trace.
All D-Bus, configuration and command effects use private fixtures. The initial
fixture with an incorrect Python bound-default path was rejected; metadata
confirmed the original host configuration path was absent. Ledger, raw
responses, provider calls, rejected candidates and source identities are under
`.omo/evidence/carrot-server-225-resume/bluetooth/`.
The pre-existing immediate-unpolled scan cancellation difference is tracked in
[#235](https://github.com/bin9208/openpilot-rust/issues/235); its bounded
first-poll fix and actual immediate/yielded comparisons are recorded in
[the Bluetooth record](rust-bluetooth-155.md).

Screenrecord ports the original catalog, three-second cache, pagination,
thumbnail, video and download routes. It reuses FileResponse for conditional
and partial responses and adds only the feature's preset headers. Actual owned
files and FFmpeg invocations cover 28 policy pairs, 51 independent HTTP pairs,
five Application pairs and nine provider-command pairs. Original and native
FFmpeg produce identical 320-by-240 JPEG bytes. A year-one local-date failure
was corrected using the original previous-day fold probe; an encoded static
route-prefix failure was corrected without decoding the dynamic ID boundary
early. Two final raw controls verify encoded-prefix success and encoded-slash
rejection. Unaffected earlier comparisons are reused with source identities.

The retained screenrecord routing-fix example SHA256 is
`5ffe0693d3c303db67b0a9e8235909ec554ee0f53483defb324fad3f360e1f63`.
Its per-gate binaries, receipts, rejected candidates and six owned-file hashes
are in `.omo/evidence/carrot-server-225-resume/screenrecord/receipt.json`.
FFmpeg remains an external provider; target codecs, ARM/device behavior and a
held ninety-second timeout/cancellation scenario have not been measured here.
The final adapter build, strict all-target Clippy and formatting pass. These
families do not complete the server's remaining routes and background tasks.

Before checkpointing, the central route dispatcher, Bluetooth status response
and Python fixture controls were extracted into separate modules. All ten Rust
function bodies, including the entire ordered dispatch, and all eleven Python
function ASTs are identical across that move (`bluetooth/refactor/identity.json`).
The final Bluetooth example
`acbd00a5a3ff5ba56e1b5412280be05b3223d44360dda426a6b7ba9c2cc7ad13`
passes four actual Application normal/error/recovery pairs. The final
screenrecord example
`48ac7c31ece8b0d9e217479788dbeb63959a8ae5bc32f802f44a2d54a6654090`
passes five Application pairs and two provider observations. Earlier family
proofs are reused for this mechanical move; strict all-target Clippy, formatting
and diff checks pass again. The Bluetooth ledger and 23-file source freeze retain
the final binaries and focused invocations.

## Dashcam catalog prerequisites (2026-10-08)

The route/segment catalog, source-file selection, path helpers and recent-read
state now have a native implementation. Ninety-two owned source/file pairs
pass with example SHA256
`ba203ba45764f83c481fcd9e6085bd1c593461e509176049d96be17c6e5c4402`.
They preserve numeric modern/legacy order, lazy positive-only timestamp caching,
invalidation and page seeds, canonical media/log preference, required-rlog
summaries and original per-entry completeness behavior. Read-state comparisons
retain exact state/temporary bytes, partial UTF-8 failure output, exception
classification, rename errors and recovery. A trace records zero child-path
filesystem access during name-only enumeration on this host filesystem.
The full receipt and seven owned-file hashes are in
`.omo/evidence/carrot-server-225-resume/dashcam-catalog/receipt.json`.

Existing path, Unicode-digit, compact UTF-8 and I/O-error helpers are reused
through visibility-only exports. The existing manifest whitespace compactor is
also shared without changing its quote/escape logic or HTML escaping; one
original/native index HTTP response matches after extraction. Its receipt is
under `shared-primitives/` beside the catalog evidence. Bounded builds, strict
all-target Clippy, formatting and diff checks pass. HTTP pagination/read-state
routes, replay/encoding, upload orchestration and full startup remain separate
work; these tests use synthetic metadata files, not actual recordings or devices.

## Git state prerequisite (2026-10-08)

The native Git state store preserves the original compact ASCII JSON, fixed
temporary filename, flush/fsync/replace ordering, pull-time conversion and
20-event history. Thirty-eight source/native cases with 89 observations per
side pass, including exact bytes, replacement failure/recovery, malformed state,
Unicode/nonfinite values and event-field projection. A syscall trace observes
fsync before rename on both implementations. The retained example SHA256 is
`0b5c62f9624f97b917a46b21bfb3475e5fdac6a58824d79ac0a1b3dddda05c88`.

The source duplicate `status` argument fails before filesystem access; the
native entry point now rejects that duplicate at the same boundary. Only clock
values are normalized in the owned filesystem comparison. Evidence, source
hashes, build, strict Clippy and formatting logs are under
`.omo/evidence/225-git-state/`; `verification.txt` records the exact invocation.
Concurrent fixed-temporary-file writers and injected fsync/permission failures
were not measured. Git status HTTP and startup/cleanup integration remain open;
this prerequisite does not implement automatic-update policy or reboot behavior.

## Git status HTTP and server lifecycle (2026-10-08)

The production executable now creates the original Git status service using
its sibling `openpilot-process-child`, starts the existing eight-second initial
delay/60-second loop and cancels/awaits that work during shutdown. Listener
accept errors now pass through cleanup before returning. The explicit fixture
constructor keeps Git inactive unless an owned service is supplied.

Forty-one independent HTTP pairs and nine final Application pairs preserve
ordered response bytes, first-value force/refresh query parsing, cache command
counts, normal/busy/error/fetch recovery and real Git state reads. A separate
persistent-connection error probe verifies the original 500 response and EOF.
The final Tools fixture SHA256 is
`2a7e92e78c1aa45083f07dfa83e49f20398db7d711be8db7363788e934d12f76`.

Original/native loop checks observe the first Git command at 8.135/8.049 seconds
and held-job cleanup at 1.065/1.015 seconds with both child and descendant exit
confirmed. Three additional native controls cover actual EMFILE accept failure
cleanup, startup validation failure and an inactive fixture constructor. The
long loop checks reuse the retained earlier binary with unchanged Git bodies;
the final nine HTTP pairs and error EOF check run after the parallel Dashcam
wiring. Build, strict Clippy and formatting pass. Exact invocations, source
hashes and limitations are in `.omo/evidence/225-tools-git-status/LEDGER.md`.
Automatic-update policy and the other startup tasks remain separate work.

## Dashcam catalogue/read-state HTTP (2026-10-08)

GET/HEAD route lists, segment pages and recent completed segments, plus
GET/HEAD/POST read state, now use the actual Application router. The native
cache retains the original directory signature and 300-second boundary,
cached-empty distinction, positive timestamp invalidation and previous index
after a failed rebuild. Pagination, newest-incomplete-tail hiding, query bounds
and read-state file/error behavior retain the original semantics.

Seventy-four independent pairs (65 endpoint and nine fixture phase checks),
six Application pairs and two persistent-connection error/recovery pairs pass.
The latter found and corrected a native 500 that left the connection open:
both servers now send Connection: close, reach EOF and accept recovery on a
new connection with matching file bytes. Final example SHA256 is
`e5cd8e6afe60168f4cf6bac6d701315094297dffe15b93983c6fef1e29fe677a`.
The unchanged 92 prerequisite comparisons are reused with checked identities.
Receipts, the failing candidate and source hashes are under
`.omo/evidence/carrot-server-225-resume/dashcam-http/`.

These checks use owned metadata/state files and localhost sockets. They do not
exercise real recordings, codecs, upload recipients, NAS, devices or stalled
filesystem cancellation. Report, summary, replay-source, media and upload routes
remain open. The complete server startup/log-upload gate is still unfinished.

## File response timestamp corrections (2026-10-08)

The metadata-route comparison exposed two shared FileResponse differences.
A file at `mtime_ns=-1750000000` returned native 500 while the original served
200 with a signed hexadecimal ETag and a 1969 Last-Modified date. At
`mtime_ns=1700000000000000001`, Python's floating-point `st_mtime` rounds to
`1700000000.0`; native nanosecond comparisons instead produced a one-second
later header, 200 instead of 304 for If-Modified-Since and 200 instead of 206
for the tested If-Range request.

The shared helper now preserves exact signed nanoseconds in ETags and uses
the original floating-point timestamp representation for header rounding and
date comparisons. Existing positive ETags/date formatting, parser behavior,
condition precedence, range logic and post-open metadata refresh are retained.
Seven focused source/native pairs pass: negative 200/304/206, the positive
one-nanosecond boundary's header/304/206 and an ordinary positive response.
Other static-family comparisons are reused without a full replay.

The selected build, strict all-target Clippy, formatting and diff checks pass.
Source identities and commands are in `.omo/evidence/225-file-timestamps/`;
raw failing and corrected wire captures are in the Dashcam metadata evidence.
The correction adds no timeout, tolerance, dependency or user option.

## Dashcam metadata and raw-file HTTP (2026-10-08)

GET/HEAD summary-source, replay-source metadata, replay-source files and
download routes now use the Application router. The original qlog/rlog/video
selection, skipped segments, numeric segment order, no-follow metadata,
signed millisecond floor, URL quoting, cache headers and download disposition
are retained. Download MIME lookup reads the original system MIME file list
for the permitted artifact suffixes, with lazy initialization and retry after
read failure; it does not hard-code this workstation's MIME database.

Sixty-three independent source/native pairs and 14 Application/phase pairs
pass. Owned files cover empty and missing artifacts, source preferences,
negative and rounded-positive timestamps, GET/HEAD, range/conditional
responses, permission recovery, MIME defaults/overrides and an unhandled
MIME error that closes the connection before fresh-connection recovery.
The shared timestamp repair's seven focused comparisons and unchanged
catalogue/read-state proofs are reused. Final Application example SHA256 is
`35c1377ec7eed54a7f16730b164fccd90026bd528bf5e9d1b90fc8ab7e5fec67`.
Commands, comparison rows and source identities are recorded in
`.omo/evidence/carrot-server-225-resume/dashcam-metadata/receipt.json`.

These routes stream untouched files. Codec generation, report parsing,
upload orchestration and complete server startup remain separate work.
No device, NAS or external upload recipient was used.

## Active Dashcam source boundary (2026-10-08)

`openpilot/selfdrive/carrot/server/features/dashcam/__init__.py` registers only
`routes.py` and explicitly leaves the legacy replay scan/query/cache handlers
off. Replay computation belongs to the existing browser assets. The legacy
`replay.py`, `replay_events.py`, `replay_index.py`, `replay_query.py`,
`replay_schema.py`, `replay_stats.py` and their private dependencies remain
inactive and unported; the Rust server must not activate their old endpoints.
The active summary/replay-source file routes, media, report and upload routes
remain part of the runtime conversion. Browser assets retain their original
implementation under the approved runtime design.

## Standalone heartbeat service (2026-10-08)

The native heartbeat service now reproduces the original payload, status
snapshot, 30-second loop, 3.5-second socket timeout and cancellation behavior.
Thirty-two source/native scenarios pass: 23 protocol and five lifecycle cases
reuse the unchanged earlier implementation evidence, and four corrected short
response cases run on the final example SHA256
`05ca171f88056ce3fbbb5a5ba79628837cf5f480bb4615c253fed5dcfcc30679`.
The cases include owned HTTP/TLS redirects, raw compressed and replacement-
decoded bodies, errors, missing/invalid Params, 800-character truncation,
second-IP lookup, initial/unavailable status and real 30-second cadence.
An advancing response lasting about five seconds succeeds despite the
3.5-second socket timeout, matching the original absence of a total deadline.
Cancelling the loop leaves its active blocking request to finish, while the
heartbeat snapshot remains unchanged.

Initial comparisons exposed three short-body diagnostic mismatches; a fourth
case confirmed the distinction between completed chunk data and its trailing
CRLF. The HTTP provider now exposes its already-parsed raw length and an
opt-in completed-chunk counter. The locked `ureq-proto` 0.6.4 source is vendored
with its licenses and provenance solely to expose existing decoder state.
No package version, parser or default read policy changes. Two real TCP tests
cover eight default/opt-in observations, including the default decoded gzip
body and raw wire length. Selected builds, strict all-target Clippy, formatting
and diff checks pass.

The scenario reuse map and source/binary identities are in
`.omo/evidence/225-heartbeat/standalone-freeze.json`; provider changes and checks
are in its `chunk-framer-seam/` sibling directory. All network recipients and
IP probes were owned/local fixtures. Application startup/status wiring and
complete runtime startup/upload remain pending at this checkpoint.

## Heartbeat Application lifecycle (2026-10-09 KST)

The production listener now owns heartbeat startup and cancellation when native
Params are available. The status endpoint always exposes the initial snapshot;
fixture constructors remain inactive unless supplied an owned startup provider.
Eight saved Application observations pass: four source/native comparisons for
unavailable Params, success, HTTP error and active-request cleanup, plus four
native controls for constructor gating and startup/accept-failure cleanup.
The source startup/cleanup heartbeat statements execute unchanged; unrelated
startup families are explicitly omitted from these focused comparisons.

The retained Application example SHA256 is
`adb848805d5165cdabf75407d8e86e77d716aaff262cfe2d024e89f8ea6f80ae`.
Cleanup completes the serve task while its active blocking heartbeat request
survives until the owned recipient is released. Process exit was sampled after
that release, separately from task cancellation. Exact commands, source/ELF
bindings and limits are in `.omo/evidence/225-heartbeat/application-freeze.json`.
The saved selected build, strict Clippy, formatting and diff checks pass and
were verified after the reboot without rerunning the completed cases.
The standalone 32-scenario comparison remains reused. Whole-server startup
and the full runtime/upload gate remain unfinished.

## Git configuration repair and pinned pull target (2026-10-09 KST)

Native `repair_git_config` and `prepare_git_pull` now reproduce the original
selected-remote/upstream repair and commit-pinning operations. They preserve
intentional differently named and local upstreams, duplicate/inherited config
errors, custom fetch mappings and the advertised-head verification. The caller
retains the repository transaction lock; only its explicitly borrowed lock FD
is inherited by captured child commands. Separate stdout/stderr, replacement
UTF-8 decoding, newline/whitespace handling and negative signal return codes
retain the source behavior.

Thirty-six paired cases pass: 34 baseline comparisons and two focused
permission-error controls. The latter corrected error attribution for a
non-searchable working directory while preserving the `git` filename for a
non-executable Git program. This attribution runs only after launch fails.
The actual original 15-second timeout is reused with a native 15.0057-second
comparison; the direct child is reaped while an owned descendant may survive,
matching POSIX pipe-close behavior. HEAD, index, working files and the prepared
target remain preserved across the controlled configuration operations and an
unrelated FETCH_HEAD update.

The final example SHA256 is
`fc0e60f2a8cfd48bf60fe7bd9f95284680ada9370662a8e433bd79cc583f65b0`.
Selected builds, strict Clippy, 19 library tests, formatting, Ruff and diff
checks pass. Source/binary identities, exact invocations and reuse boundaries
are in `.omo/evidence/225-git-config/checkpoint/receipt.json`.

One preliminary fixture setup lacked its own Git repository and inadvertently
fetched the current development checkout's configured remote. Its output showed
no config repair, and branch/upstream/working-file readback remained unchanged.
That setup failure is recorded separately and is not validation evidence. The
actual corpus checks an independent `.git` and exact repository root before
each source/native invocation and uses owned file remotes. Automatic-update
orchestration, recovery CLI/reset/merge and stale-index handling remain separate
work; the runtime candidate has not been deployed.

## Dashcam thumbnail, preview and browser video (2026-10-09 KST)

The active thumbnail/preview/video routes now use the Application router and
the shared source-compatible FFmpeg capture boundary. The implementation keeps
positive-cache reuse, raw segment cache tokens and attachment names, thumbnail
seek fallback and the exact placeholder SVG, preview-to-thumbnail fallback,
MP4 passthrough and TS remux behavior. Audio is copied first; an incompatible
audio stream falls back to video-only MP4, then to the original TS if necessary.

Forty-seven HTTP pairs, 40 command-argument pairs and two persistent-socket
cases match the original. Actual external FFmpeg comparisons retain identical
JPEG/GIF/remux bytes and ffprobe observations for AAC preservation and the PCM
fallback. The Application checks cover eight route cases, Internal500 close/EOF
and normal MP4 keepalive. The retained final example SHA256 is
`f8d255ff0253bc14a7cb2d393310e19390fe8c3d4f6fbe940c43f5f224698cc2`.
The portable `rust/tools/carrot_server_dashcam_media.py` entrypoint has a
two-case relocation smoke check; its unchanged scenario bodies reuse the
completed family evidence. Selected build/strict Rust gates pass.

The scenario-to-artifact map, exact commands, source identities and captured
codec outputs are in
`.omo/evidence/carrot-server-225-resume/dashcam-media/media-checkpoint/receipt.json`.
The common 90-second capture deadline is reused from #242; the original
120/180-second parameters are retained without claiming additional wall-clock
timeout experiments. Inputs are owned synthetic media. Route-report aggregation,
upload orchestration and complete server/runtime startup remain open.

## Dashcam upload HTTP jobs (2026-10-09 KST)

The Application now serves upload summary, start, job polling and cancellation
through the existing native upload library. A persistent owner thread keeps the
worker alive across requests and releases it when the server closes. The upload
library's catalog, state, transport and worker bodies are unchanged.

Fifty-two original/native HTTP response pairs match: 32 request boundaries,
11 cancellation/shutdown lifecycle responses, seven composed Application
responses and two normal-completion responses. Normal upload sends the owned
4,096-byte video and 1,024-byte log with matching hashes and authorization;
the terminal result, error, progress and byte counters agree. Cancellation and
server shutdown observe recipient disconnects and actual worker exit. A partial
fixture-start failure also cleans up both peers and receiver threads.

Packaging must include the sibling `openpilot-dashcam-upload` executable.
Four native-only requests verify that its absence produces an explicit upload
500 while the Application, unrelated Params API and upload summary continue.
This packaging error is separate from the original asynchronous transfer-error
path. All recipients are owned loopback servers; no NAS or vehicle was used.

Selected build, strict Clippy, library tests and formatting pass. The retained
example SHA256 is
`e6610ff19fd13309b70b9f8f90a924ca82d06f73b0e67d589ee41589f203e372`.
The portable driver is `rust/tools/carrot_server_dashcam_upload.py`;
commands, source identities, normalization limits and scenario artifacts are in
`.omo/evidence/carrot-server-225-resume/dashcam-upload-http/checkpoint/receipt.json`.
Synchronous upload and driving-route report aggregation remain separate work.

## Dashcam upload connection check (2026-10-09 KST)

The Application now serves `POST /api/dashcam/upload/test` through the existing
native upload health and automatic-session helpers. Metadata lookup is deferred
until a healthy target needs a session. It uses the resolved runtime repository
and only the relevant environment keys; unrelated non-UTF-8 environment data
does not break the request.

Seventeen original/native paired executions match: 12 standalone request and
helper cases, three composed Application cases and two held-recipient shutdown
cases. Healthy, unhealthy and invalid-target responses match while an unrelated
Params route remains available. During shutdown, both original and native
servers wait for the held health/session request; releasing the owned recipient
produces the matching response, recipient EOF and server exit zero.

The original non-UTF-8 environment failure and corrected result are retained.
The 12 standalone cases are reused for Application integration; no full upload
corpus was repeated. Existing transport timeout/TLS evidence remains separate.
The portable driver is `rust/tools/carrot_server_dashcam_health.py`; the scenario
map, exact build and source identities are recorded in
`.omo/evidence/carrot-server-225-resume/dashcam-health/checkpoint/receipt.json`.
All HTTP recipients and repositories are owned local fixtures. This establishes
neither NAS access nor device/runtime acceptance.

## Automatic-update conditions (2026-10-09 KST)

The native pure conditions preserve the original manager-readiness interval,
sample-gap reset, park/disengaged/offroad reboot rules, verified-target gate
and Unicode error-detail suffix. Twenty original/native cases with 820 repeated
inputs match, including invalid inputs and exact time boundaries. The portable
`rust/tools/carrot_server_auto_update_policy.py` driver accepts the built
`auto_update_policy` example and an output directory.

Selected build, strict server Clippy, 19 library tests and formatting pass.
The retained policy example SHA256 is
`19c6ee998253d89a7e3fb4b0b2c461baa917568835396ea6246dcbb458e11eba`;
commands and the scenario map are recorded in
`.omo/evidence/225-auto-update-policy/checkpoint/receipt.json`.
This checkpoint does not yet connect manager IPC, automatic Git transactions,
notifications or the reboot request. Those remain separate integration work.

## Automatic-update Git transaction (2026-10-09 KST)

The native `_run_git_pull` transaction now performs the original pinned reset
and fast-forward merge through the existing captured-process runner. It retains
the caller's repository lock across commands, alert callbacks and an awaited
notification callback. The Git status reader and command runner are unchanged.

Twenty original/native cases match results, command arguments, state transitions
and exact stored JSON bytes. Owned repositories cover a dirty reset to the
selected commit, divergent or unchanged heads, active index locks, state-save
failure, failed HEAD verification, duplicate reboot protection, process errors,
callback failures and cancellation. Independent lock contenders verify exclusion
and release, including after an awaited callback.

The cancellation fixture waits for the actual owned reset descendant to become
ready before canceling. Its missing-readiness control fails within its two-second
bound and performs the existing one-second process-group cleanup. A forced
harness timeout also retains its original error while releasing children and
locks. Nineteen unaffected cases were reused after this fixture-only correction.
The production 10/120/180-second command limits remain unchanged; no additional
full-duration timeout experiment is claimed.

Strict all-target Clippy and 19 library tests pass for the frozen production
sources; the final example passes its selected Clippy and formatting checks.
The portable driver is `rust/tools/carrot_server_auto_update_pull.py`. Exact
source identities, commands and scenario results are in
`.omo/evidence/225-auto-update-pull/checkpoint/receipt.json`; the retained example
SHA256 is `f62948b154a15a50a26b67d6cb905c0a5315eb9486063dfc9b4d0ecc3bf3c27b`.

This independent transaction takes clock, alert and notification recipients from
its caller. Manager monitoring, update-attempt scheduling, real notification,
reboot and Application lifecycle integration remain open. All Git mutations in
these comparisons target owned temporary repositories.

## Conservative repository preparation (2026-10-09 KST)

The native repository preparation helper preserves the original index-lock
recovery rules: a regular file at least 60 seconds old, no observed Git process,
and an unchanged device/inode/mtime/size after the 100 ms recheck. Unavailable
process inspection defers recovery. Git path lookup retains its ten-second
deadline and strict, separate text decoding before interpreting the result.
The caller's existing repository lock stays held until blocking inspection
finishes, including when cancellation is requested.

Twenty-seven original/native cases match results, file state, messages and Git
arguments. They cover the age boundary, file type, process visibility, changed
or removed locks, malformed command output, actual Git process observation,
the real lookup timeout and cancellation. Protected cooperative/ref/config
locks remain unchanged; child cleanup and lock exclusion/release are observed.

The selected build and strict Clippy, formatting, diff and Ruff checks pass.
The source runner retains asyncio for the original cancellation/shield behavior;
that optional skill-audit exception is recorded explicitly. The portable driver
is `rust/tools/carrot_server_repo_recovery.py`. Commands and source identities
are in `.omo/evidence/225-repo-recovery/checkpoint/receipt.json`; the retained
example SHA256 is
`66979e10cbe710b185a42551cf352940db99fcea3f9645b040dff5541611b211`.

All mutations target owned fixtures. Update-attempt coordination, manager
monitoring, reboot and complete Application startup remain pending.

## Synchronous Dashcam upload checkpoint (2026-10-09 KST)

The independent `POST /api/dashcam/upload` adapter uses the existing upload
worker protocol for each call. It preserves simultaneous synchronous requests
and coexistence with the asynchronous job API; these calls do not create or
replace entries in `DashcamUploadJobs`. A persistent owner thread keeps each
admitted worker alive after an HTTP client disconnects. Admission precedes body
parsing and remains owned while a call waits for the bounded command channel.

Twelve paired HTTP responses match the original: seven method/input/result
boundaries, two simultaneous uploads, and three start/sync/cancel responses
while an asynchronous upload is held. A separate paired status observation
checks that the held asynchronous job remains present. Three native lifetime
controls match the retained original captures: client disconnect, server stop
with a connected client, and disconnect followed by stop all retain the held
upload until release. Each completed upload delivers the selected 4096-byte
and 1024-byte files and notifications, then closes the recipient connections.
A separate native force control checks
the actual worker and its owned Git descendant, including process identity,
exit notification and worker reaping before recipient requests begin.

The standalone source identities and results are retained under
`.omo/evidence/carrot-server-225-resume/dashcam-sync/`, particularly
`standalone-source-freeze.json` and the `standalone-{boundary,concurrency,
lifetimes,startup-force}-v1` directories. These results reuse the existing
engine, transport and asynchronous-job evidence. The worker environment
correction is recorded separately in
[`rust-dashcam-worker-environment-249.md`](rust-dashcam-worker-environment-249.md).

The adapter is now connected to Application routing and the existing single
60-second shutdown grace period. Six composed HTTP responses check successful
upload and missing-helper isolation: unrelated Params requests remain 200
around either a 200 upload or an explicit 500 packaging error. The existing
`openpilot-dashcam-upload` helper must be packaged beside the server. Three
composed disconnect/stop lifetimes match the saved original captures with no
differences. Immediate Drop also terminates the owned worker and Git child.

A focused Tokio control reproduced a shutdown wait cycle when a seventeenth
sender already owned a pending reservation on a full 16-entry channel. Closing
and awaiting the receiver's end could wait for that sender while the sender's
executor was blocked joining the owner. Forced shutdown now drains only
available commands and drops the receiver before joining workers. The old API
sequence remained blocked until killed; the corrected sequence joined in
1.90 ms. This is a causal channel-operation control, not a production
saturation run.

One actual grace-expiry run kept a disconnected client's held upload alive
through 59.8587 seconds, then exited successfully at 60.1094 seconds with the
worker reaped and recipient connection closed. There is no second grace window.
The selected App build, strict all-target Clippy, 19 library tests, package
formatting and diff checks passed. Final identities and reproduction paths are
in `dashcam-sync/checkpoint/receipt.json` beneath the evidence directory above.
Whole-server completion, branch CI, normal startup and device acceptance remain
separate gates.

## Automatic-update attempt coordination (2026-10-09 KST)

`auto_update_pull::Update` now composes the original readiness and verified
status checks, 300-second cooldown, locked branch/HEAD revalidation, repository
recovery, Git configuration and pinned-target Pull. The same cooperative lock
stays owned through these operations. Cancellation during blocking Git
configuration waits for that operation before releasing the lock. Busy outcomes
retain the original waiting state, and the status cache is cleared on every
exit from the locked attempt, including errors.

The portable source comparison runs 23 paired cases with 28 update calls.
These include every readiness checkpoint, branch/HEAD changes and failures,
configuration failures, idle/pulling lock contention, dirty repositories,
299.999/300-second boundaries and cancellation while configuration holds the
lock. Results, state transitions, Git arguments, cache probes and child cleanup
match the source. Existing Pull and recovery comparisons were reused.

Selected strict Clippy, scoped rustfmt, Python Ruff/syntax and diff checks pass.
The current example SHA256 is
`af139933258dbf5906d7cf6b2422c38c9b29e51fb97ec7454b91f3459cfeee74`;
the current checkpoint is `.omo/evidence/225-auto-update-attempt/checkpoint/receipt.json`.
The monitor loop, reboot/notification integration and Application lifecycle
remain the next implementation stage. This is not a complete update service or
whole-runtime candidate.

## Driving-report source compatibility (2026-10-09 KST)

The unchanged source report does not count the current numbered
`driverDistracted1/2/3` and `driverUnresponsive1/2/3` warning names: its category
still lists the older names plus `tooDistracted`. A synthetic full-cereal
sequence confirms that `driverDistracted1` is unmatched while `tooDistracted`
is counted. Evidence is retained in
`.omo/evidence/carrot-server-225-resume/dashcam-report/original-state-v1`.
The port preserves this membership. The inherited reporting defect is tracked
separately in [#252](https://github.com/bin9208/openpilot-rust/issues/252);
changing warning semantics is outside the current compatibility conversion.
This observation is not vehicle-log evidence.

## Driving-report HTTP integration (2026-10-09 KST)

The native report now reads raw, zstd and bzip2 logs with the full cereal schema,
retaining cross-segment state, warning/disengagement/corner aggregation,
excursion merging and limits, source selection and original numeric formatting.
Its 51 standalone comparisons include an unfinished zstd stream: the original
preserves the decoded prefix on unexpected EOF, while checksum and malformed-tail
errors still fail. The native decoder follows that distinction.

Nine HTTP source/native comparisons pass for default and qlog selection, HEAD,
duplicate query values, segment aliases, missing routes, POST rejection, NaN and
an owned-root permission error. Six Application responses preserve normal report
results and unrelated Params access before and after either success or a 500.
The permission-error comparison normalizes only the owned fixture root in the
body and its corresponding Content-Length delta; captured wire bytes remain exact.
All owned source/native processes exited successfully and cleanup checks passed.

The selected examples and Application were built; strict all-target Clippy,
19 library tests, formatting, Ruff/syntax and diff checks pass. Existing unchanged
standalone and upload evidence was reused. The source comparisons are available
through `rust/tools/carrot_server_dashcam_report*.py`; current results are under
`.omo/evidence/carrot-server-225-resume/dashcam-report/`.
Only `bzip2` 0.6.1 and `libbz2-rs-sys` 0.2.5 were added to the lockfile, together
with a dependency edge to the already locked zstd 0.13.3. Other versions remain
unchanged. Whole-server and normal-startup integration remain open.

## Current dev composition (2026-10-09 KST)

`d2f5a6a46cbe69a8a7435840a3eb6f5164fbfa7d` combines the committed server
features with validated dev `bbed1c5432e8b1f2d8974fd8b001e93feaee456e`, including
the interrupted logger connection correction. Merge resolution preserves dev's
Athena pyzmq dependency and single CarrotMan gate entry, plus the server workspace
member and patched providers. Locked offline metadata resolves 91 packages and
all 22 CI-isolation tests pass. The composed App was rebuilt in the separate
integration checkout, then its six report/Params responses passed against the
existing source references. No unchanged full corpus was repeated.
[Exact-commit Fast checks](https://github.com/bin9208/openpilot-rust/actions/runs/37894118462)
also pass. Required whole-server CI and complete startup remain later gates.

## Automatic-update runtime (2026-10-09 KST)

The native service now owns the continuously sampled manager readiness monitor,
existing locked update transaction, post-update reboot monitor, offroad alert and
CWP notification. Messaging objects stay on the local runtime thread. The
manager observer continues during blocked Git operations; a manager restart
therefore resets the ten-second readiness interval. Reboot requests retain the
existing vehicle-state conditions and persistent duplicate-request receipts.
Alert and DoReboot writes retain the existing Cython Params boundary: filesystem
return codes are discarded, while key and encoding failures remain errors. This
is confined to these server recipients; the shared Rust Params API is unchanged.

Seven source/native live scenarios pass with actual msgq, owned Git repositories,
loopback notification recipients and local Params. They cover disabled updates,
the verified update through park-triggered DoReboot, manager invalidation during
held fetch, fetch cancellation/reaping, shielded configuration completion and
lock retention after stop, native Application cleanup during configuration, and
notification Git cancellation after the updated state is persisted. The
Application case compares native server cleanup with cancellation of the original
auto-update coroutine; it does not run the original complete web Application.
Persisted behavior, recipient requests and cleanup match. Command traces remain
available but are not included in the whole-loop equality comparison. A separate
actual-msgq scenario verifies subscription retry, readiness and invalid reset.
Results are under `.omo/evidence/225-auto-update-runtime/live-service-v1/` and
`live-manager-v1/`; all owned children and repository locks are released.

Notification comparisons include the UTF-8 payload, ten-commit cap, diff counts,
redirects, HTTP errors, timeout and incomplete bodies. Seven earlier wire cases
are reused; the explicit HTTP/1.1 truncated-chunk case also matches the source
failure. An earlier fixture sent HTTP/1.0 with Transfer-Encoding: chunked, which
is faulty framing under [RFC 9112 section 6.1](https://www.rfc-editor.org/rfc/rfc9112.html#section-6.1).
The differing external-provider response to that malformed framing remains an
explicit limit in `http10-provider-limit.json`; no provider patch or claimed
matching result was added for it. These owned checks contact no CWP deployment.

## Remaining work

The live-family source comparison also reproduced an inherited navigation
snapshot defect, tracked separately in
[issue 254](https://github.com/bin9208/openpilot-rust/issues/254). With a populated
actual-msgq `NavInstruction` message, the original API returns null `mainText`,
`distanceText` and `turnType`: the snapshot reads those nonexistent Cereal fields
instead of the `maneuver*` fields. The port preserves this observed output.
The retained HTTP capture precedes the unfinished idle-control stage in
`carrot-server-225-resume/live-runtime/whole-v4/family/`; it is evidence for this
specific source defect, not whole-family completion or vehicle behavior.

Profiles, restoration and change-history services have independent process
evidence, and profile/history HTTP routes and real index bootstrap are connected.
Multipart restoration has isolated and composed HTTP evidence; QR dependency
status/repair remains in progress.
The request decoder has the tested expanded charset/compression coverage above;
broader original codec aliases and provider error diagnostics remain explicit
limits. Multipart extended names use the existing encoding_rs provider for
supported labels.
The live broker plus raw/compact/camera WebSocket transport are in progress. The remaining active
families include system actions, network/calibration/time-sync services, tool
jobs, terminal/support terminal, Carrot Navi's web bridge, YouTube Live and
vision diagnostics/test services. The web bridge is distinct from the converted
Carrot Navi daemon. `/stream` also depends on the separately inventoried WebRTC
conversion. Each family's existing guards, background tasks and cleanup belong
to the same conversion scope as its registered HTTP routes.
The executable now resolves runtime assets using existing OPENPILOT_ROOT/BASEDIR
environment conventions or executable/working-directory ancestors, without a
build-checkout fallback. Eight relocated CLI cases pass; this exercises the
partial listener, not the unfinished whole-app startup/background services.

Complete original server behavior, required branch CI, the small dev connection
check and full manager startup with the existing log-upload path remain gates.
The user performs the first device comparison only after the complete runtime
candidate is ready. These host observations establish no CPU savings or vehicle
acceptance.
