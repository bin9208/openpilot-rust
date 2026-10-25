# Optional decoder SDK

This directory builds the nine pinned external libraries in `sources.json` as
static Linux decoding dependencies. No Pillow library, Python interpreter,
LLVM or Meson is used by this build. `dav1d.cmake` lists the sources from dav1d
1.5.3's `src/meson.build` and builds the portable C implementation for both
8-bit and high-bit-depth input. It intentionally retains decode semantics while
not claiming SIMD performance. libyuv uses its original CMake build and retains
its native optimizations. libavif has only dav1d decoding enabled, no encoders,
apps or tests. OpenJPEG's apps, tests and docs are disabled. TIFF uses pinned
JPEG, zlib, liblzma and zstd libraries; its tools and optional system codecs are
disabled. All source archive hashes and upstream licenses are retained.

Compile `stage.rs` with Rust 1.94.0 and pass the archive directory, fresh build
directory, fresh installation directory and this SDK source directory. Set
`CHECK_DISK_SPACE_BINARY` to the repository's disk gate. The stager checks both
destination filesystems before extracting, verifies all nine archives and uses
two build jobs. It rejects existing or overlapping destinations. CI provisions
the same sources through `.github/actions/native-image-sdk`.

`IMAGE_CODECS_SDK_ROOT` points Cargo at the installation. The receipt hashes all
nine installed libraries; Cargo checks it before compiling the codec boundary.
An absent, incomplete, changed or relocated receipt is rejected.

The fresh stager and production decoder tests passed on Linux x86_64 on
2026-10-18, including 410 TIFF captures, JPEG2000 signed/varied precision,
palettes, tiles, MCT and raw codestreams, AVIF range/subsampling, and every chroma
pair at five luma levels. The TIFF C boundary passed 5,502 sanitizer attempts
linked to these static libraries; the external libraries themselves were not
sanitizer-instrumented. This is bounded host evidence, not exhaustive codec or
device validation. The final image-array gate also passed all 85 independent
review regressions; broader repository conversion remains tracked in #296.

The crate requires `native-sdk`; its empty default feature set neither links
codecs nor requires `IMAGE_CODECS_SDK_ROOT`. Production polygon checking requires
`image-array-native` so it cannot silently fall back to a reduced decoder set.
EPS rendering separately uses the system Ghostscript executable.

The libyuv archive uses the full pinned commit from the GitHub mirror because
Googlesource's generated tarballs embed request-time file timestamps and do not
have stable archive hashes. The mirror archive was compared against the original
Googlesource capture: all 204 paths, file bytes, links and executable bits match.
Reconstructing its Git tree yields `ba22b636166dc16532fcc9d5a9524117432d1782`,
the tree reported for commit `644251f252a84bf8ce91ff0aca86a9b16b069ab8` by both
origins. Repeated mirror downloads have the same pinned SHA256; the SDK strips
the mirror archive's single root directory just like the other sources.
Fresh downloads of all nine manifest URLs passed their archive pins after this
change. A clean SDK stage, all six stager/receipt tests, five native codec tests,
16 production image/polygon/CI tests and scoped strict Clippy passed using that
new installation on Linux x86_64. Archive pins remain mandatory and fail closed.
