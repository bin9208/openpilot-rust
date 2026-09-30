# Route logger native dependencies

Project source provenance is the repository's original loggerd, VideoWriter,
ZstdFileWriter and encoders, under their existing licenses. Original C++ msgq
is linked through the existing boundary and is not relabeled as a Rust rewrite.

Rust bindings are locked by `rust/Cargo.lock`: ffmpeg-next 8.1.0 and its resolved
ffmpeg-sys-next dependency use WTFPL; zstd and zstd-sys carry their upstream
license notices. The host validation links the installed FFmpeg development
libraries. Generic cross-builds compile the original FFmpeg 6.1.1 release archive
from `https://ffmpeg.org/releases/ffmpeg-6.1.1.tar.xz`, SHA256
`8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968`.

The reproducible recipe is `rust/tools/build_loggerd_ffmpeg.py`. It retains the
complete original source archive/tree, configuration command and build log in
its output directory. That configuration enables neither GPL nor nonfree
components. FFmpeg's `COPYING.LGPLv2.1` and `LICENSE.md` accompany cross-build
evidence artifacts. All original source notices remain in the extracted tree.
The source, recipe, Rust source and locked bindings permit rebuilding/relinking
the isolated artifact. No generic artifact is installed on a vehicle here.

Raw HEVC storage uses the OS C library's stdio through locked `libc` Rust
bindings to retain the original buffering and error behavior. The Rust wrapper
owns each FILE allocation once and does not share it across threads. The OS
libc implementation remains an external dependency.
