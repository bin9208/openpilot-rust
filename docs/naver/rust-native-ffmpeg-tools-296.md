# Native FFmpeg provider builders — issue #296

On 2026-10-14, `openpilot-dev-tools` replaced the maintained
`rust/tools/build_loggerd_ffmpeg.py` and `build_runtime_ffmpeg.py` entry points
with native `build-loggerd-ffmpeg` and `build-runtime-ffmpeg` binaries.
The implementation is `rust/crates/dev-tools/src/ffmpeg_build.rs`; both CI
logger-provider callers invoke the Rust binary. These binaries do not launch
the removed Python implementation or another interpreter adapter.

The original FFmpeg 6.1.1 source remains external native code, pinned to
SHA256 `8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968`
from `https://ffmpeg.org/releases/ffmpeg-6.1.1.tar.xz`. The logger profile retains
the original minimal static codec/muxer/parser/bitstream-filter selection without
GPL enablement. The shared runtime profile retains libx264/libvpx/OpenSSL/zlib,
CLI/ffprobe, GPL and version-3 enablement. Source notices remain intact, and the
appropriate original LGPL-2.1/GPL-3 license plus `LICENSE.md` is installed.
GNU make, target compilers/binutils, pkg-config, configured external codec
libraries, and curl for an uncached download remain external build dependencies.
The existing musl CI compiler command still uses the pinned external Zig package;
this change does not retire the other CI Python fixture/oracle families.

Both profiles enforce 1–2 jobs and the 25 GiB reserve plus 3 GiB estimated growth.
The Rust tool authenticates an owned frozen archive, uses checked extraction,
rejects altered cached sources, records commands/results/hashes, and verifies
actual little-endian AArch64 compiler predefines. Installed public headers must
match the authenticated source or generated target build headers (`avconfig.h`
and `ffversion.h`). `--headers-only` configures and installs those headers without
building provider libraries. It does not establish complete FFmpeg functionality.

## Validation

- Six focused native library tests passed, covering both original configure
  profiles, SDK/pkg-config environment, licensing, job limits, build failures,
  corrupt/modified source, header provenance and host/big-endian rejection.
- Two native CLI admission tests passed, covering invalid/duplicate flags and
  jobs before output creation, and unpinned archives with an empty executable
  search path before any external compiler can run.
- The two native builder binaries compiled under Rust 1.94.0 with the canonical
  coordinated Cargo cache, incremental compilation disabled and two jobs.
- A real pinned-source minimal logger build used the current AGNOS target
  compiler: configure, AArch64 predefine probe, `make -j2`, and `make install`
  all passed. Its completed record contains 123 verified public headers and 157
  installed file hashes, including `libavcodec.a`, `libavformat.a`, `libavutil.a`
  and license records. An actual AAC encoder object was identified as little-endian
  AArch64 ELF. The real build exposed the generated `ffversion.h` provenance case;
  the implementation and regression fixture were corrected before final success.
- A separate fresh native `--headers-only` run passed configure, actual target
  probing and `install-headers`, verified 123 public headers, and created no
  provider library directory.
- Focused formatting and Git whitespace checks passed.

Local reproduction records are under
`.analysis/scratch/2026-10-14-native-ffmpeg/`; they are ignored working evidence.
The standalone SDK currently lacks the supplemental libvpx headers/pkg-config
input used by the historical shared-provider build. The full shared runtime
profile was therefore not rebuilt in this validation; its recipe/environment
is covered by native tests. This is developer-tool/provider build evidence,
not vehicle testing, GPU validation, CPU savings, or the complete runtime gate.
