# Rust WebRTC runtime (#240)

[#240](https://github.com/bin9208/openpilot-rust/issues/240) tracks the standard
`webrtcd` and Carrot Vision WebRTC runtimes within the full conversion in
[#1](https://github.com/bin9208/openpilot-rust/issues/1). The current host
checkpoint covers real HTTP, native Cereal/msgq, ICE/DTLS/SRTP and ordered SCTP.
Startup composition, target packaging
and exact-head CI are still pending. This is
intermediate engineering evidence; no device or performance claim is made.

## Ownership and external dependencies

Rust owns request/session policy, the original transport-owner graph, source
activation order, answer metadata, encoded-camera scheduling, H.264 packetization,
Cereal bridges, compact-state cadence, RTP sender history/reporting and cleanup.
Source contracts are `openpilot/system/webrtc/{webrtcd,carrot_webrtcd,
carrot_session,carrot_state}.py`, `device/video.py`, the local `teleoprtc` package
and pinned aiortc 1.14.0. Their original licenses are retained.

The native provider is rtc 0.21.0 with AWS-LC 1.18.1 / aws-lc-sys 0.45.0.
The owned provider copy retains its published manifest/licenses and provenance.
Its limited patches select the negotiated BUNDLE master, prepare/defer a single
owner until actual activation, preserve source-selected local DTLS roles and
forward real DTLS close-notify before transport removal. Each original transport
owner is a separate provider peer; no submitted client SDP credentials or media
attributes are canonicalized to force BUNDLE. Ordinary provider role derivation
is unchanged when the source-selected role is absent.

## Saved host evidence (2026-10-09)

Private artifacts are under `.omo/evidence/240-webrtc/` in the primary checkout.
The current sender checkpoint is `production-sender-checkpoint.json`, which
binds source hashes, binary hashes, invocations, captured outputs and remaining
work. The selected native package build, strict all-target Clippy and four unit
checks are captured in `production-srtcp-delivery-final/`.

- Preserved owner controls cover negotiated BUNDLE, partial/no BUNDLE, original
  discarded-owner references and candidate eligibility, sticky local role and
  sequential owner activation. Carrot's real 12-second negotiation pruning,
  13-second connected retention and wire DTLS close-notify were observed.
- H.264 packetization and PTS controls retain 11 bitstream cases and four actual
  source msgq cases. Two SRTP cases match 37 packets and two decoded frames each,
  including MID then ABS header-extension order. Live clocks/random identities
  are recorded separately from controlled source payload/header comparisons.
- Four original/native sender-loop cases retain normal, empty-payload,
  malformed-bitstream and buffered-channel behavior. CVF1 uses the initial RTP
  timestamp zero and marks its source ID before callback. An empty-payload frame
  may refer to the previous RTP timestamp at the next receive, as in the source.
- `production-http-srtcp-regression-invocation.json` exercises both actual native
  service paths after the sender correction: 37 received packets/two decoded
  frames, full Cereal JSON, inbound native msgq, notify, Carrot CVF1/Params and
  peer closure before caller cleanup. Owned listeners can be rebound.
- The session captures retain constructor failure without replacing the old
  peer, same-client replacement, Carrot busy/takeover and answer failure after
  replacement. The separate guarded HTTP control compares all 16 status/error/
  CORS/Allow results and every parsed schema field; schema JSON whitespace/key
  order is not claimed byte-identical.
- A usable application-first MAX_BUNDLE offer connects and exchanges data in
  both implementations. The distinct-credential BALANCED application-first
  input exposed the provider STUN-error gap addressed in the scoped #259
  correction below; the original accepted input and failure are preserved.
- `production-audio-readiness-bridge-invocation.json` covers four real offered
  audio cases. Incoming sendonly audio is unconsumed, holds the Cereal bridge
  unready, and still permits notify. No audio playback/capture is activated.

## Sender feedback correction

`production-srtcp-loss-red/` drops one encrypted RTP datagram before the owned
receiver's SRTP unprotect call, then sends an actual SRTCP NACK. The unchanged
source recovers the packet. The initial Rust sender did not. This control avoids
the invalid earlier test that requested an already received packet and was
hidden by SRTP replay protection.

Two separate causes were fixed. The provider's default interceptor did not
deliver inbound sender RTCP to the application; a project-owned public
`DeliverToApplication` marker now enables the original 128-slot retransmission
cache. Separately, rtc-rtcp's SDES chunk marshaler emitted a padding count byte
where the source requires null octets. The actual decrypted compound packet
failed aiortc parsing. [#256](https://github.com/bin9208/openpilot-rust/issues/256)
tracks that native dependency defect. The project emits source SDES bytes using
the public `RawPacket` API without modifying the provider. The live result in
`production-srtcp-delivery-wire-invocation.json` verifies loss recovery and parsed
SR/SDES/BYE, source packet/octet counters and CNAME identity, and BYE/close while
the transport is still live. Negotiated RTX wrapping has a focused unit check;
its actual negotiated wire path is verified by the debug controls below.

## Generated debug video

The native track preserves aiortc's 640x480 YUV420 frames with all input plane
bytes zero, its wall-clock cadence, and its original lazy VP8/H.264 encoder
settings. It uses a separate native codec worker and the same RTP history/report
path as encoded cameras. The public media registry contains the pinned source's
codec set; VP9/AV1 provider defaults are not advertised. VP8's source codec
time-base conversion yields initial RTP deltas `0,2999,6000,9000`; H.264 yields
`0,3000,6000,9000`. These observed values are preserved.

Three actual source/native HTTP pairs cover standard VP8, Carrot VP8 and
negotiated H.264. Each native case decodes twelve frames, receives actual RTX,
responds to PLI and REMB, and closes the peer before caller cleanup. Visible
decoded plane hashes match the first five original frames. Zero input planes
do not imply zero decoded planes through the lossy codec: an initial fixture
assumption failed and is recorded separately. Losing the initial H.264 SPS/PPS
packet also requires a fresh PLI keyframe before decoding resumes in the
original; the corrected fixture records that boundary explicitly.

[#258](https://github.com/bin9208/openpilot-rust/issues/258) records the provider
API boundary found by the actual loss control: `write_rtp` accepts only primary
SSRCs and rejects the negotiated RTX SSRC. The project uses the public protocol
write path for RTX after checking its negotiated apt, SSRC and extension IDs.
Primary RTP keeps its existing provider guards. The initial source/native RED
and actual corrected wire captures are indexed by `debug-checkpoint.json`.
The reusable driver is `rust/tools/webrtc_debug_compare.py`, with owned Params
roots, explicit IPC namespaces and loopback ICE. Its helper/style gates and
tracked-driver wire check are in `debug-driver-final/`.
After the registry change, a native sendonly-audio connection matches its saved
original control: the peer/channel connects, the unconsumed audio keeps the
Cereal bridge unready, notify is delivered and the native process exits zero.
`debug-audio-registry/` records that focused regression; the source corpus was
not repeated.

These host controls use the existing FFmpeg 6.1.1 native libraries (avcodec
60.31.102, libvpx 1.14.0) through ffmpeg-next 8.1.0; the original PyAV 16.1.0
environment uses avcodec 62. This is an explicit external dependency difference.
Compressed debug bitstreams are not claimed byte-identical. Target native
dependency packaging and exact cloud checks remain separate acceptance work;
the later owned browser controls are recorded below.

## Shared compact encoding

`openpilot-carrot-state` mechanically extracts the four frozen Live
codec/schema/value files from commit
`3d55bd71f931fd10e0d82f5b1b22eaf922905783`, preserving diagnostic display.
`shared-compact-checkpoint.json` records the source-to-destination hash mapping.
The source's saved 21-service encodings match the new crate's bytes, without a
source recapture. Actual standard HTTP/SCTP delivery also matches all 21 service
packets and subsequent sequence increments. WebSocket intervals stay in the
server; RTC preserves its own intervals, 16 KiB gate and last-send advancement
before a buffered drop.
The primary server now connects these reexports at
`d07befaf345012c2f14164d147a4f2c8215d641a`, with 21 saved encoding comparisons
and one actual compact WebSocket check. This shared-code integration does not
merge the RTC runtime or its provider into the server branch.

## Boundaries and local fixture side effect

All wire recipients are owned loopback peers. Tests disable default external
STUN through the existing fixture provider; production network behavior is not
validated by these controls. Original HTTP tests invoke real source sessions,
but omit the unavailable Python Params binding and original CLI startup.

The subsequent `params-source-native-bytes-invocation.json` closes the Params
ownership gap using the existing unchanged Cython binding, explicit owned Params
roots and an isolated logging path. One actual original/native Carrot road
connection persists exact ASCII `0,1,0` and closes the peer before caller
cleanup. The original uses its real nonblocking writer; the native process uses
runtime `PARAMS_ROOT`. The first observation incorrectly compared the binding's
typed `get` result with bytes and timed out while the actual source file already
contained `0`; that failure is retained. The corrected control observes the
persisted bytes directly. Original CLI/default-root and affinity startup remain
separate work.

An earlier boundary-only native fixture omitted `PARAMS_ROOT` and wrote ASCII
`0` to the workstation's `~/.comma/params/d/CarrotVisionActive`. Its previous
value is unknown because Params replaces files atomically. The host file was
preserved. `host-params-side-effect.json` records the exact invocation/path and
uncertainty. The shared fixture launch now fails closed unless its Params root
is inside the dedicated evidence directory and an explicit IPC namespace is
present. Only the affected 16 HTTP checks were rerun with verified owned paths.

## Owned mDNS and server-reflexive transport

The project resolver follows aioice 0.10.2's actual query/answer boundary;
`rust/crates/webrtc/AIOICE-LICENSE` retains its BSD license. No new dependency
or vendor visibility patch is used. It resolves selected remote candidates
after SDP preparation and before answering, without rewriting the submitted
description. Candidate and owner resolution are concurrent. Queries coalesce
only while pending; each waiter retains the source one-second timeout.

Eleven actual original/native UDP cases match query bytes, reply bytes, results
and empty waiter state. They cover A/AAAA, case-insensitive names, duplicate
waiters, uncached repeated queries, cancellation, close, compressed names,
first-answer selection, malformed/truncated/class/opcode rejection, and the
source's acceptance of QR/AA-unset answers with a nonzero ID. Native owner
sockets can be rebound after cleanup. `mdns-source-native-replay.json` binds
the captured source cases and native test executable.

The HTTP adapter initially serialized two missing names: source answered in
1.011 seconds and native in 2.032 seconds. The retained
`mdns-http-serial-red-invocation.json` records this failure. A scoped concurrent
candidate adapter now answers in 1.031 seconds against source 1.009 seconds.
That negative case proves answer timing and query behavior only. The actual
duplicate-name session emits one query in both runtimes, connects ICE/DTLS and
the ordered data channel, delivers `/notify`, and closes the peer before caller
cleanup. See `mdns-http-concurrent-invocation.json`.

`srflx-owned-nat-first-invocation.json` exercises both runtimes through owned
STUN and UDP NAT aliases. The client retains only advertised server-reflexive
candidates and drops direct base-address datagrams before handling. Both
nominate the real alias, forward datagrams in both directions, deliver data and
close before caller cleanup; native exits zero. The provider's existing
`base_addr()` routing works, so no additional mapping or SDK change is needed.
These are loopback controls, not a claim about external STUN or a deployed LAN.

Remaining work includes startup composition, target dependency
packaging and exact-SHA CI. No C3X, NAS or vehicle handoff has
been performed.

## Browser, HTTP text and CLI checkpoint

`browser-media-first-invocation.json` and `browser-carrot-first-invocation.json`
drive owned Chrome 155 with unchanged BALANCED offers and genuine UUID.local
candidates. Source/native each receive 37 RTP packets / 45,529 payload bytes and
decode three 128×96 frames with equal RGBA hashes and relative RTP timestamps
0/4500/9000. Carrot's three binary CVF1 tuples exactly match those browser frame
timestamps; actual Cython-owned Params bytes are ASCII 0→1→0. Notify is delivered,
browser errors are empty and native exits zero. Before caller closure Chrome
reports PC connected, DTLS closed, track live and data channel closed in both
cases; this is not a claim that Chrome's PC became closed.

`body-codec-green-invocation.json` compares 15 actual HTTP cases and delivered
SCTP JSON against the saved source capture. Request-declared strict decoding
uses the existing pinned `rust/vendor/charset-norm` 3.5.1 provider, including
UTF8-sig/UTF16/UTF32/BOM, Latin1, CP1252 and EUC-KR cases. CP1252 byte81 and
EUC-KR8141 are rejected; EUC-KR's composed-Jamo encoding of 힣 returns the same
syllable. Unknown encodings and route-specific decode failures preserve source
status behavior. No detection or replacement fallback is used, and every Python
codec is not claimed covered.

`cli-affinity-green-invocation.json` runs five actual original/native Carrot
CLI pairs with valid, nonnumeric, negative, large and Unicode/underscore core
values. PC startup preserves inherited affinity after integer parsing;
CPU-set/range validation and the syscall remain in the source `/TICI` path.
Actual default `Params()` uses owned PARAMS_ROOT, schema responds and SIGTERM
exits zero. This host control does not establish target/device startup.

Reusable drivers are `rust/tools/webrtc_{browser,body,cli,stun}_compare.py`,
`webrtc_browser_peer.mjs` and their narrow helpers. Run them in the recorded
source-oracle environment. Browser inputs use WEBRTC_BROWSER_FRAMES,
WEBRTC_PARAMS_BINDING and WEBRTC_OMOWRIGHT_ADAPTER; profiles, listeners, Params
and IPC namespaces remain owned. The original scratch launch hashes and final
tracked helper identities are retained; mechanical/style moves did not repeat
the network corpus.

## Opt-in STUN check correction (#259)

[#259](https://github.com/bin9208/openpilot-rust/issues/259) records two concrete
provider gaps: rtc-ice discarded error responses before transaction handling,
and silently discarded bad request usernames instead of returning source400.
The pinned owned rtc-ice0.21.0 copy enables original aioice check semantics only
through the deferred/source-owned RTC preparation path. Ordinary provider mode
retains its upstream policy. No arbitrary timeout or whole-peer400 shutdown is
introduced. Submitted SDP and source credentials remain unchanged.

`stun-error-pairs-red-invocation.json` preserves the actual source/native RED.
`stun-error-http-green-invocation.json` reuses that source capture and observes
native client/ICE/DTLS failure: its pair changes from IN_PROGRESS at0.050535s to
FAILED at0.059453s. Transaction/local-owner matching, late/unknown/wrong-local
handling, one failed pair with another viable pair, real checklist exhaustion,
signed bad-request replies,487 retry and pending nomination failure/fallback
are exercised by eight actual owned UDP controls in `stun-provider/`.
The nomination fallback first failed by choosing recipient0 again, then reaches
Connected through viable recipient1 after the narrow pointer correction.

`browser-carrot-stun-regression-invocation.json` and
`srflx-stun-regression-invocation.json` retain nominal browser/CVF1 and forced
srflx transport after the initial check correction. Their d885ac66… ELF predates
the final nomination-only pointer fix. Reuse is explicit: those paths never
enter that unselected nomination-error branch; they are not represented as runs
of the final ELF. Final build/strict gate, eight controls, source/ELF identities
and license provenance are frozen separately in
`.omo/evidence/240-webrtc/stun-provider/final-freeze.json`.

## Client identifier ownership

`identifiers-red-invocation.json` records four actual original/native HTTP
ownership sequences: compound client/device values and lone surrogate strings
return source200 versus the former native500. The native adapter now reuses
`openpilot-runtime-version::python_str` and retains code points in its internal
client key, including trimming and the 128-code-point limit. Carrot's original
ASCII identifier normalization remains separate.

`identifiers-final-invocation.json` reuses that source capture and matches all
four complete sequences. Standard D800/D801 keys remain distinct and replacing
D800 closes only its prior peer; compound values match their Python string
representation. Carrot compound device precedence and surrogate normalization
replace the original owner. Peer/channel states are captured before caller
cleanup. The source HTTP subset leaves Params binding inactive; actual Cython
Carrot 0→1→0 parity is covered by the earlier separate ownership control.

## Production multi-camera and FIR routing

`production-multicamera-first-invocation.json` runs the actual standard and
Carrot HTTP services with simultaneous driver/road/wideRoad queues. Distinct
encoded bitstreams and rotating publication order preserve each camera's MID,
payload hashes, four marker timestamps and three decoded RGBA frames. Track
order matches the requested driver/road/wideRoad order. Carrot road relative
timestamps are 0/4500/18000/45000 while the other clocks remain sequential;
exactly four road CVF1 tuples match the received road RTP timestamps. Source
and native peers/channels close before caller cleanup.

[#261](https://github.com/bin9208/openpilot-rust/issues/261) records FIR and
compound RTCP routing mismatches. aiortc selects FIR by media_ssrc and considers
every packet in a compound datagram. Native formerly used FIR entry SSRCs and
gated the vector on its first packet. A default-false source-mode endpoint flag
now selects the first routable SSRC across that vector, uses FIR media_ssrc and
delivers the vector once. Project sender guards still select each recipient;
ordinary provider routing retains its prior behavior.

`fir-routing-red-invocation.json` and `fir-compound-wire-red-invocation.json`
retain the real encrypted feedback/keyframe failures. The native-only
`fir-routing-green-invocation.json` reuses both captured source subsets and
matches all five observations: matching FIR, zero media SSRC, mismatched entry,
empty RR before FIR and the reversed compound order. Build and strict native
Clippy pass. The multi-camera proof uses c882a01e…; the FIR correction uses
52e69a11…. These are distinct binaries. The prior multi-camera media/clock
observations are reused because they contain no FIR or an unroutable leading
RTCP packet. Native/aarch64 CI and AGNOS dependency packaging are separate gates.

## Native CI and integration boundary

The `webrtc-runtime` job in `.github/workflows/rust.yml` selects the native
feature on Ubuntu 24.04 x86_64 and ARM runners and is required by the aggregate
Rust check. It builds both service binaries and the two owned examples, runs
strict native Clippy and the five native unit checks, then compares actual
ownership/media/FIR/body/Params/CLI/srflx boundaries, plus one selected original/
native H264 debug pair for libx264, NACK/RTX/PLI/REMB. Python oracle packages are
pinned in `rust/tools/webrtc_source_requirements.txt`; binding provenance, native
package versions, architecture, SHA and outputs are artifacts. Browser captures
and unchanged historical corpora are reused separately.

The first native CI run at ea3302e8b, [37950081007](https://github.com/bin9208/openpilot-rust/actions/runs/37950081007),
passed native compilation, unit tests and strict Clippy on both architectures,
then failed importing the original hardware chain because the declared Python
requirements omitted pyserial. The repair pins pyserial 3.5 and verifies all 35
declared packages in a fresh CPython 3.12.14 environment. Original hardware,
WebRTC entrypoint and CI-helper imports pass without hardware stubs or the old
broad site-packages path; the four actual identifier ownership sequences pass
using the existing 52e69a11… native ELF. The same run exposed package-formatting
differences and an inherited Card aggregate assertion missing webrtc-runtime.
Only the reported formatting and required-job assertion are changed. Workspace
formatting, the focused Card assertion and 23 isolation-policy checks pass;
exact-head cloud results for the repair remain pending. Logs, invocations,
clean package list and file identities are in
`.omo/evidence/240-webrtc/ci-first-repair/final-freeze.json`.

The job requires the FFmpeg 6.1.1 / libavcodec60 ABI, libvpx9 and libx264-164
encoders. Ubuntu publishes [libavcodec-dev](https://packages.ubuntu.com/noble/libavcodec-dev),
[libvpx9](https://packages.ubuntu.com/noble/libvpx9) and
[libx264-164](https://packages.ubuntu.com/noble/libx264-164) for amd64 and arm64.
This Linux runner coverage does not establish AGNOS deployment compatibility.
The target package must supply compatible FFmpeg avcodec/avformat/avutil and
libvpx/libx264 native libraries, their transitive dependencies and licenses;
AWS-LC builds from the pinned Cargo source. Existing logger-only cross libraries
do not enable the debug VP8/H264 encoders. Initial CI preparation reused local
providers; its dependency repair installs only the pinned source-oracle closure
into a separate clean environment.

The adjacent original `selfdrive/carrot/server/features/stream.py` remains the
server owner's integration boundary: raw POST bytes and Content-Type go to
fixed `http://127.0.0.1:5001/stream`, with total five-second timeout,
ClusterHud409, timeout504/network502 and diagnostic events. This read-only audit
does not implement that primary server proxy. Full manager startup, log upload,
target packaging and user device acceptance remain global delivery gates.
