# Continuous route storage and media validation

Issue [#41](https://github.com/bin9208/openpilot-rust/issues/41), under full
runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).
Source: unchanged `openpilot/system/loggerd/loggerd.cc`, `logger.cc`,
`video_writer.cc`, `zstd_writer.cc` and native encoder implementations.
Original source licensing and [external dependency records](../../rust/crates/loggerd/THIRD_PARTY.md)
are retained. This increment does not select a production daemon.

`openpilot-loggerd` subscribes to original service queues without conflation,
preserves ordinary message bytes, decimates qlog with per-service counters across
segment rotations and converts encoded messages into original index messages.
The new msgq API separates readiness polling from single-message dequeue so the
source's 200-message per-service drain bound works without losing a packet at
each boundary. Services retain the source's lexical ordering. Other subscribers
keep their existing conflation policy.

Each route has a persisted counter/random identifier, initData, route/segment
sentinels, zstd-compressed rlog/qlog and incomplete-file locks. CurrentRoute,
RouteCount, user.preserve and AthenadRecentlyViewedRoutes updates follow source
ordering. initData preserves original vehicle/version/Params identity, redacts
DONT_LOG values and adds explicit Rust implementation, actual build SHA and
clean/dirty/unknown build-tree entries. No environment override fabricates the
build SHA. Kernel/OS/command observations are sampled from the running host.

Four encoded streams coordinate normal rotation. First-seen offsets, late
segment handling, the 201-message maximum pending queue, waiting for audio and
keyframe gating remain. Timeout fallback requires strictly more than 60 seconds
and either more than 500 ms without a camera packet or a segment longer than
72 seconds. The timer begins after new-segment I/O, matching the source even
when opening/finishing files is slow. LOGGERD_TEST retains the source behavior
that disables these fallbacks. No runtime timing threshold is loosened.

Full HEVC payloads use a uniquely owned libc stdio wrapper to retain the original
buffering, EINTR and write/flush/close behavior. FFVHUFF uses Matroska and H.264 uses
MPEG-TS; codec extradata, packet timestamps and frame durations retain source
behavior. Qcamera audio preserves 16-bit sample conversion, mono AAC at 32 kbps,
queueing, padding/flush and packet interleaving. FFmpeg and zstd remain native
dependencies; project-owned orchestration and file lifecycle are Rust.

SIGINT/SIGTERM/SIGPWR record the actual signal in the final sentinel. SIGPWR
retains the source sync call. Successful finalization removes locks; malformed
inputs or fatal finalization failures retain incomplete locks. Raw video writes
follow the source's nonfatal behavior: actual ENOSPC can still leave indices and
a removed video lock after exit 0. This inherited reporting/completion behavior
is tracked separately in [#54](https://github.com/bin9208/openpilot-rust/issues/54).
The Rust port does not silently change upload or retention policy to fix it.
With RecordAudio enabled but
no audio ever received, the original VideoWriter crashes at trailer creation;
Rust returns a typed failure and retains the empty video's lock. Both finalize
rlog/qlog first. This inherited defect remains open as
[#48](https://github.com/bin9208/openpilot-rust/issues/48); no successful video
finalization is claimed for that case.

## Observed validation

The parent native run uses original C++ logger/encoder binaries and actual
Python msgq peers with temporary directories and synthetic Params. Its complete
schema and decoded-media comparisons pass:

- Ordinary scenario: 810 rlog and 251 qlog packets with original decimation.
- A 1,000-packet queued CAN burst retains all messages. A paused multi-service
  burst verifies lexical service ordering and the 200-message fairness boundary.
- Three video segments retain all four streams, preserve requests, indices and
  route/segment sentinels. FFVHUFF/H.264/HEVC frames, packet timing and AAC
  decoded samples compare exactly to original outputs; ambient container
  metadata is inspected separately from media content.
- Audio startup, overflow and encoder restart cases, disabled recording,
  unsubscribed streams, malformed packets and all three signals pass.
- Real fallback observations from process launch: original/Rust no-camera
  rotation 60.144/60.205 seconds; active-but-stuck encoder 72.156/72.181 seconds.
  The harness enforces [60,62] and [72,74] seconds without shortening the source
  waits. These observations include the slow-I/O clock correction.
- A source review found the Rust rotation timer sampled before file I/O. An
  injected-clock regression failed before the correction and passed after it;
  the subsequent complete native/media/fallback run passes. Miri also passes
  the three pure rotation-policy tests; native FFI is checked separately.
- Actual strace comparison exposed three Rust-only fsync calls at rlog/qlog
  and raw HEVC close. They are removed to preserve the source's flush/close
  contract. Rlog/qlog close failures remain checked through a narrow ownership
  boundary. Storage/media regression and final integration checks cover this
  correction separately from the earlier timing observations.
- A real `/dev/full` raw-video case first exposed a fatal direct-write Rust
  difference. The stdio correction matches original exit, index packets and
  video-lock removal while strace confirms actual ENOSPC in both processes.
- Worker native AddressSanitizer and generic aarch64-musl build/emulated media
  checks passed. Parent final-head checks and required cloud gates are separate;
  these do not establish device execution or performance.

Independent generated values are checked and normalized narrowly: route IDs,
timestamps within observed intervals, ambient `df -h` output and the explicit
Rust provenance additions. Ordinary message payloads, encoder index fields,
Params redaction and media content are not masked. The original native log
fixture currently replaces only external cloudlog transport with captured
stderr. It does not prove source diagnostic producer compatibility.

Reproduce from the repository root using native FFmpeg development libraries,
capnproto, libyuv and the existing oracle Python environment:

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-loggerd --locked
cargo test --manifest-path rust/Cargo.toml -p openpilot-msgq --test queued --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-loggerd --locked
python rust/tools/build_msgq_python.py --output /tmp/logger-msgq
python rust/tools/loggerd_native_build.py --output /tmp/logger-original
PYTHONPATH=/tmp/logger-msgq:.:rust/tools python rust/tools/check_loggerd_native.py \
  --binary rust/target/debug/openpilot-loggerd --original /tmp/logger-original/original-loggerd \
  --producer /tmp/logger-original/original-encoder-producer --output /tmp/logger-native --edges --hevc --fallbacks
```

Private captured evidence remains in `.omo/evidence/issue41/` and the parent
analysis scratch directory. The required Rust gate now depends on a dedicated
native route-logger job; GNU/musl ARM builds use the checksum-pinned FFmpeg
recipe. The full project runtime, manager startup, diagnostic callsites, upload
and first device comparison remain open. No vehicle was accessed and no CPU or
thermal saving is claimed.

Docs-Not-Needed: isolated Rust runtime implementation and host verification;
no production selector or user setting changes.
