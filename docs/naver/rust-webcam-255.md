# Optional webcam runtime conversion: issue 255

[Issue 255](https://github.com/bin9208/openpilot-rust/issues/255) tracks the
registered `webcamerad` process within the complete-runtime gate in
[issue 1](https://github.com/bin9208/openpilot-rust/issues/1).
The original entrypoint is `openpilot/tools/webcam/camerad.py`, with capture and
pixels in `camera.py`; their last source change is
`d89dda4751c5b49b77a85db662328eb1ca434138`. Original project licensing is retained.

`openpilot-webcam` is the independently built Linux daemon from
`rust/crates/webcam`. Its `native` feature uses a small project CXX capture bridge
and external OpenCV/libswscale. Camera selection, rotation, packed pixels,
publication, timestamps, scheduling and ownership are Rust. No Python runtime is
called by the daemon. Manager selection and its webcam/driverview predicate are
unchanged; normal manager startup and target packaging remain integration gates.

## Source behavior and ownership

Linux always prefixes `/dev/video` to the raw `ROAD_CAM`, `WIDE_CAM` and
`DRIVER_CAM` values, including string IDs. Road defaults to 0; nonempty optional
values select wide then driver. This differs from the reusable source Camera
class's integer-or-string constructor. The supported native provider/package
here is Linux; Darwin selection has a pure test, not a native macOS claim.

Capture requests 1280×720 at 25 Hz and reads actual width/height/FPS. Packed BGR
frames rotate 180 degrees before NV12 conversion. Each selected stream has 20
VisionIPC buffers and an independent 20 Hz ratekeeper, anchored after its first
publication. Preserve `int(frame_id * 0.05 * 1e9)` floating-operation order for
both SOF/EOF. VisionIPC valid is false; CameraState valid is true and contains only
frame ID and identity transform, with all other source fields left at defaults.
CameraState follows VisionIPC publication.

The source joins independent Python workers. An exception prints but does not
cancel siblings or propagate out of main. The Rust coordinator holds the existing
non-Send VisionIPC/PubMaster owners; each capture stays exclusively on its worker
thread and waits for publication acknowledgment before incrementing its ID.
Natural EOF releases capture immediately. Failed captures remain owned until
Camerad teardown, matching the source reference lifetime. Typed external failures
replace Python exception formatting; no source traceback text is synthesized.

Zero/odd camera extents remain advertised without publishing a fabricated frame.
The opt-in msgq file-storage seam has no image mapping or publishing handle for
these streams. Both 3×6 and 4×3 fail on their first publication while a healthy
sibling continues. The original client accepts their descriptors; the existing
Rust client's invalid-layout rejection stays intact.

A source-admitted 2×2 stream publishes 6-byte NV12 frames, although the existing
ordinary producer allocator requires 8-byte-aligned pixel length. A separate,
webcam-only owned-file producer writes exact payload then native-endian ID bytes
with `FileExt::write_all_at` before the existing queue publication. It never
constructs an unaligned typed pointer. Ordinary mapped producers and client
validation are unchanged. The existing Rust client's byte-atomic tail handling
accepts/copies this layout; the actual ID and six bytes are verified.

## Native providers and provenance

OpenCV 4.13.0 is pinned to
[`fe38fc608f6acb8b68953438a62305d8318f4fcd`](https://github.com/opencv/opencv/tree/fe38fc608f6acb8b68953438a62305d8318f4fcd).
Archive SHA256 is
`6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61`.
Reuse the existing two defined-access hardening patches in
`opencv-runtime/native/opencv-defined-access.patch`, SHA256
`7d17439697d9786b43b2a44dc9eecea34381adb6d2f14c6892266b6699a49aec`.
`build_xiaoge_opencv.py --prepare-only` prepares that exact source on a fresh
runner; `build_webcam_opencv.py` builds only core/imgproc/imgcodecs/videoio,
FFmpeg/V4L2 and bundled JPEG, with two jobs. Tests, examples, Python, DNN,
GStreamer, GUI and optional download providers are disabled. The local source is
reused read-only; existing Xiaoge build/install and FFmpeg SDK are untouched.
An initial unused ADE configuration download is recorded; the builder now
explicitly disables ADE. Sealed headers/libraries and OpenCV, libjpeg-turbo and
zlib notices accompany its provider receipt.

The local FFmpeg 6.1.1 ABI is avcodec 60.31.102, avformat 60.16.100,
avutil 58.29.100 and swscale 7.5.100. The task-owned SDK overlay adds only exact
Ubuntu `libswscale-dev` version `7:6.1.1-3ubuntu5`; its package SHA512 is
`11034b04d1bbd3efe10eff53cca5c630d197328412d695668be01211f3404efab5035eb22bce1d0cc54bb192405fe3e1463c6958140a28a082629fa737a2d07b`.
These providers, libstdc++, libc, Linux V4L2 and shared-memory APIs remain external
dependencies. Host/aarch64 CI is distinct from an AGNOS-compatible target package.
The original SCons binding selects `visionbuf_ion` when `/dev/ion` exists.
`USE_WEBCAM` is not limited to PC, so this is remaining target integration under
issue 255. The host proof uses POSIX
storage; target ION feature selection and import of webcam file-backed edge
layouts remain explicit packaging/interoperability gates. No ION client guard
is weakened and no target device is contacted.
The existing later control is `check_msgq_ion.py` with `msgq_ion_fixture.cc`,
the original `msgq/native/ion_contract.cc` and native `examples/ion_contract.rs`.
It observes owned allocation/share/import/cache/free operations and handle/mapping
cleanup without `/dev/ion` access. A feature-specific safe producer must be
validated there before this boundary can be considered covered; DMA-BUF `pwrite`
is not assumed to work.

The original oracle uses CPython 3.12.14, OpenCV Python 4.13.0.92 and PyAV 16.1.0,
whose libswscale 9 disables half-chroma input for odd RGB widths. Native
libswscale 7 uses the public `FULL_CHR_H_INP` flag only for odd widths to select
that source behavior. Normal dimensions keep BILINEAR. See primary provider
[libswscale 9 condition](https://github.com/FFmpeg/FFmpeg/blob/n8.0/libswscale/utils.c)
and [libswscale 7 input](https://github.com/FFmpeg/FFmpeg/blob/n6.1.1/libswscale/input.c).
The packed output follows PyAV's ceil chroma geometry and final ndarray reshape,
rather than rejecting every odd extent. Exact requirements are recorded in
`rust/tools/webcam_source_requirements.txt`; original Cython/C++ IPC bindings are
test oracles only.

## Owned comparisons and bounded reuse

Actual command outputs and raw captures are under the private primary checkout's
`.omo/evidence/255-webcam/`; they are not committed or uploaded to a vehicle.

| Scenario | Actual observable and capture |
|---|---|
| 28 pixel cases, plus five real raw-AVI frames | `camera-corrected`, `camera-padding`, `odd-final`: all bytes/source rejection behavior match; unchanged normal cases are reused across the odd-width-only correction |
| Independent workers and real original VisionIPC peer | `source-workers`, `source-workers-edges`, `native-workers-final`, `native-workers-edges-final`: road 8/wide 3 EOF, odd 3×6, odd height 4×3, missing camera and unaligned 2×2 |
| Full CameraState contract | Same worker captures compare every parsed field except separately captured `logMonoTime`; all raw serialized messages retained |
| Capture/FD ownership | Native descriptor snapshots return exactly to the three initial FDs, including the listener, 40 buffer FDs and 40 listener duplicates; failed camera FD remains live before caller cleanup |
| Linux CLI, three cameras, actual Cereal and VisionIPC | `linux-cli-first`, `linux-cli-corrected`: unchanged original module and native daemon, exact nonexistent `/dev/videowebcam255_*` aliases mapped only to owned file inputs |
| Fixed CLI receive interval | All expected IDs 1..EOF are mandatory, with strict pixels/metadata/CameraState equality and measured approximately 50 ms gaps; raw initial losses stay recorded separately |
| Capture bridge memory boundary | `wrapper-sanitizer-workers`, `wrapper-sanitizer-edges`: five actual native worker cases under ASan/UBSan; OpenCV/FFmpeg providers are not instrumented and leak detection is disabled |
| Getter exception translation and I/O-error teardown | `final-errors`: external C++ getter fault fixtures produce typed Rust errors, closed-state query follows eight exact frames, and actual observer directory-write failure exits without a worker join hang |
| Pure rules | `pure-miri`: four selection/rotation cases pass Miri; native FFI is covered separately |

The CLI recipient begins before server launch, but publisher-generation reset and
descriptor connection can lose initial messages. Source observed 15 Vision frames
and 13 Cereal messages; native observed 14 and 13. This gate compares the fixed
post-startup interval, not all 16 frames. Full worker controls separately cover
frame 0. Unobserved frame 0 publish-versus-connect ordering is qualified; later
peer captures can record actual connection clocks. Runtime startup is not delayed
for the fixture. The alias shim proves normal Linux entrypoint/selection/IPC
composition with file-backed capture, not physical V4L2 behavior.

Preserved failed attempts include incorrect odd-height rejection, insufficient
row-padding-only correction, an overly broad native-client rejection expectation
and a capture-fault fixture initially triggering during open. An observer I/O error
also exposed a worker teardown deadlock; closing the command sender before join
fixes the actual error scenario. No failed attempt is counted as passing.

One exceptional input differs deliberately: a Linux `ROAD_CAM` environment value
containing the non-UTF-8 byte `ff` reaches the original OpenCV string converter and
the source process exits with SIGSEGV. The native daemon rejects it before opening
capture with `Environment(NotUnicode("\\xFF"))` and exits 1. The raw invocation,
outputs and provider identities are recorded in `malformed-env.json`. OpenCV's
`modules/python/src2/pycompat.hpp:70–76` calls `PyBytes_Check` on the result of
`PyUnicode_AsUTF8String` without checking for null; the retained diagnostic has no
debugger stack, so this is a source-backed cause candidate. This crash is not
emulated or included in the ordinary runtime-equivalence claim. The original
provider defect is tracked separately in
[issue 266](https://github.com/bin9208/openpilot-rust/issues/266).

The `webcam-runtime` required job selects Ubuntu 24.04 x86 and ARM runners,
rebuilds pinned capture/source bindings and exercises actual pixels, independent
workers, cleanup, Linux CLI and external getter errors. Its result participates
in the aggregate gate and exact job-set policy assertions. At this checkpoint
fresh-runner CI, updated-dev integration, AGNOS packaging, physical cameras and
normal whole-project startup/upload remain pending. This is an intermediate
runtime module, not the user's first device-test handoff.
