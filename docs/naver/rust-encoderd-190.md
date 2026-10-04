# Rust encoder conversion (#190)

The approved full runtime conversion in [#1](https://github.com/bin9208/openpilot-rust/issues/1) includes all seven encoder process profiles. [#190](https://github.com/bin9208/openpilot-rust/issues/190) tracks this isolated unit. Work is on `codex/feat-190-encoderd`; no production selector or device is changed.

Original policies are `openpilot/system/loggerd/encoderd.cc`, `loggerd.h`, and `encoder/{encoder,ffmpeg_encoder,jpeg_encoder,v4l_encoder}.{cc,h}`. The Rust unit owns configuration, camera synchronization, frame/segment selection, metadata publication, codec control, thumbnails, idle/prewarm handling and lifecycle. FFmpeg, libyuv, libjpeg and Linux/QCOM drivers remain external. The temporary project-owned C++ msgq/VisionIPC transport remains explicitly unported.

Source inspection identified inherited reconnect behavior: after a new server ID or buffer index sets `VisionIpcClient.connected=false`, the encoder's inner receive loop continues receiving against old buffers and does not return to its outer connect loop. This must remain an encoder-specific retained-buffer receive path; existing model/camera clients keep their connection checks. [#191](https://github.com/bin9208/openpilot-rust/issues/191) records a host reproduction with the original Cython/C++ client and two separate original server processes: sixteen receive-only calls return null after the restart, fifteen observed new-server packets set disconnection while four old buffers remain owned, and explicit reconnect resumes the new frame. Old payloads were copied before reconnect, with no stale view access afterward. The proof exercises the transport and source receive-call sequence, not the encoder/codec executable or hardware. Its 52-artifact receipt has SHA-256 `5364a5c328683f60047208ad1f8aea64fb4a4cb26847269ebb7df74f052a392a` under the main checkout's `.omo/evidence/encoderd-restart/`. No source reconnect fix is included.

The software encoder intentionally follows the original FFmpeg behavior, including its defaults, counter-based 50 ms PTS and lack of a close flush. Native settings and V4L controls preserve all source profile values, seven input slots and six output slots. DMA mappings/FDs must remain owned until input dequeue; a simulated V4L fixture is not hardware evidence.

The host and pinned ARM codec comparisons below are complete; composition into the full runtime and exact-head CI remain open. Encoder builds use the coordinated shared Cargo cache, with a fresh disk guard before every build. Existing loggerd FFmpeg headers/libraries, libyuv archive and vendored libjpeg assets are reused.

## Host implementation checkpoint (2026-10-03)

The native encoder package and its codec probes build on Linux x86_64. Rust owns
V4L setup, ION allocation/cache synchronization, the dequeue worker, input leases,
metadata/header publication, segment drain and EOS handling. Source retry behavior
is retained: V4L ioctls retry EINTR without a bound, while ION operations use the
source's bounded retry. A failed input QBUF terminates immediately rather than
waiting for a slot the driver never accepted.

`rust/tools/check_encoder_codecs.py` compiles the unchanged source FFmpeg/JPEG
method bodies with a publication capture adapter. Five cases passed with 112
byte-identical compressed packets and matching metadata/return values, covering
lossless frames, scaling, delayed H.264 output, two segments without close flush,
and both 1928x1208 and 1280x720 thumbnail inputs. Local evidence is
`.omo/evidence/encoderd-190/codec-final/receipt.json`.

`rust/tools/check_encoder_v4l.py` compiles the unchanged source V4L methods and
ION allocation methods against a scripted host driver. All 28 cases passed:
twelve encoder configurations, both main-quality width branches, seven-slot
backpressure, EINTR, crop/readback/compatibility/slice fallback paths, rejected
dimensions/rate control/capability/offset/timestamps/QBUF, two segment drains and EOS.
Rust publication is decoded through actual native msgq and the full cereal
schema; packet metadata, headers and fixture bytes match source. Each run checks
six capture allocations/frees and all input returns. The fixture intentionally
drops each Rust caller's input owner immediately after submit, checking that the
encoder retains its FD/mapping until dequeue. Local evidence is
`.omo/evidence/encoderd-190/v4l-final/receipt.json`. This is not VIDC hardware
validation. The first attempt failed because the test had not created its private
msgq directory; that harness failure and its logs are retained separately.

Host codec proof uses the actual `6.1.1-3ubuntu5` FFmpeg runtime, with libavcodec
60.31.102, libavformat 60.16.100 and libavutil 58.29.100. Library hashes and runtime
version values are retained. The repository's pinned dependency package is
`ffmpeg==7.1.0` from dependency commit
`b9732165bcf5a3fab83b05994187802a0d115b6e`; these host libraries and the pre-existing
generic ARM FFmpeg 6.1.1 build are not the final AGNOS codec/CLI package. The
parent integration owns that external dependency closure.

`rust/tools/build_encoder_source.py` builds the unchanged original `encoderd`
executable, including its actual codec, VisionIPC, msgq and logging code.
`rust/tools/check_encoder_runtime.py` then drives the original and Rust processes
with owned native VisionIPC servers and 20 Hz synthetic frames. All ten cases
passed with 1,207 publications per implementation: all seven process profiles,
three-camera startup synchronization, server restart and overwritten-buffer
handling. Comparison includes complete normalized encode metadata, compressed
payloads and thumbnail bytes; only independently sampled wall/boot publication
clocks are checked for validity rather than equality. Raw full-schema messages,
inputs, binary hashes and invocation details are retained under
`.omo/evidence/encoderd-190/runtime-final/`.

The actual daemon restart case preserves the inherited #191 behavior: after a
new VisionIPC server starts, the receive-only inner loop publishes no new
frames. The lag case stops only its owned test daemon, fills seventy frames
into sixty-four camera buffers, resumes it and verifies both implementations
drop exactly the first six overwritten inputs. Carrot Vision performs its single
prewarm, suppresses inactive frames, resumes on the same codec and never rotates.
Normal main recording produces 130 road packets, 36 delayed qcamera packets and
one frame-100 thumbnail; the multi-camera case adds 130 packets each from driver
and wide road. Both processes exit cleanly on the test's SIGTERM.

AddressSanitizer with leak detection passed the five codec cases, all 28 scripted
V4L cases and three actual-runtime scenarios (restart, lag and on-demand, 431
publications per implementation). Rust/std, the native msgq bridge and the full
vendored JPEG codec were instrumented; the external host FFmpeg and libyuv
libraries were not rebuilt with instrumentation. Receipts are under
`asan-codecs-1/`, `asan-v4l-1/` and `asan-runtime-1/` in the same evidence root.
Miri default and strict provenance/alignment runs passed the four pure policy
tests. Miri does not execute these native codec/driver FFI paths. The four native
policy tests also passed. One Clippy warning in thumbnail bounds arithmetic was
corrected with `div_ceil`; final native tests and Clippy with warnings denied passed.

Parent review found that the original test cleanup could miss a child that had
already exited unexpectedly. The checker now requires each expected exit status,
rejects sanitizer diagnostics even in expected-failure cases, and reaps timed-out
children. Nine focused outcome checks passed, including a retained reproduction
with an already-exited status-23 child. The full five codec, 28 V4L and ten runtime
cases above were rerun with those checks against the frozen final host binaries.
The runtime receipt records the actual executable and loaded library maps.

The VisionIPC buffer frame-ID read now uses `memcpy` instead of a potentially
unaligned integer dereference. An odd-aligned owned buffer reproduced the old
UBSan failure and passed with the corrected bridge; no packet or frame layout
changed. Evidence is retained in `alignment/` under the same local evidence root.

The exact locked libyuv 1922.0 ARM wheel was staged without installing Python:
release `libyuv/v1922.0`, SHA-256
`1659e55a357f732836e5ed2a17fa65e715a11bf9bfa6d305e71fd7ac00a8e475`,
with every extracted file hashed. The parent also staged the locked FFmpeg ARM
package, SHA-256
`ce758b64c0343574e18ab97cbcff1e29868c66ccc5e0c866b5970266451203d4`,
whose native runtime reports `b08d7969`, avcodec 61.19.100, avformat 61.7.100 and
avutil 59.39.100. The final encoder daemon and both probes compiled for aarch64
with these static FFmpeg/libyuv libraries and the ION feature. The generated JPEG
configuration and archive were selected from the current Cargo build record,
then frozen with the executables in `arm-final/`.

`rust/tools/check_encoder_target.py` built the unchanged source codec methods
against those exact target dependencies and compared them with the frozen Rust
probe through QEMU and the extracted AGNOS 19.8-carrot-bt1 loader. All five cases,
112 compressed packets, complete metadata traces and encode return values matched
exactly. Both executables were verified as AArch64, and the loader resolved their
shared libraries inside the extracted AGNOS tree. This establishes offline ABI
and codec behavior, not physical V4L/ION operation or device performance.

The ARM run receipt is
`.omo/evidence/encoderd-190/arm-pinned-codecs-1/receipt.json`, SHA-256
`d40f18bbfa8262b0e7dcae86523e257a9c34044339f5d783c4e92d73e5bafdb0`.
The Rust codec probe is
`4ff80986dd6cbcec53f4e33c4697f12fa95cb61acad0cd5f1abb30ea91d6477f`;
the source oracle is
`a82de9ebe5460c8cabe7f8c7459d5debd5b48d0e8a01c9e3e492f7ac8befe23a`.
Ten checker boundary checks reject packet/trace/count/ELF/loader/exit mismatches.
The parent verified all 513 unique handoff/run artifacts and all 48 frozen source
hashes after the run. Full runtime composition and hosted CI are still pending.
No device, production selector, vehicle setting or user guide was changed.
