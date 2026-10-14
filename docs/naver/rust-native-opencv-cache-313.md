# OpenCV source immutability and generated download cache — #313

Rust run [38552326535](https://github.com/bin9208/openpilot-rust/actions/runs/38552326535)
at source `0992e61f` completed Xiaoge OpenCV configure/build/install on both
architectures, then rejected completion with
`external build changed its read-only source tree`. This is tracked by
[#313](https://github.com/bin9208/openpilot-rust/issues/313).

The Xiaoge profile omitted `OPENCV_DOWNLOAD_PATH`, while the webcam/runtime
profiles already supplied an owned output directory. The pinned OpenCV
`cmake/OpenCVDownload.cmake` defaults the cache to `${OpenCV_SOURCE_DIR}/.cache`
and unconditionally creates `.cache/.gitignore` during initialization, even
when no third-party download is needed. Thus configuration itself adds a file
to the authenticated source tree and correctly trips the strict final gate.

The fix sets `OPENCV_DOWNLOAD_PATH=<output>/downloads` in the common configure
arguments for all three profiles. No source exclusions or acceptance checks are
changed. The complete source-file/hash comparison still rejects added files and
modified existing files; user-provided source trees are not cleaned or rewritten
to conceal mutations.

The actual pinned OpenCV CMake module was executed in isolated scratch with an
owned source directory. The original default created
`control-source/.cache/.gitignore`; supplying the fixed cache flag created
`fixed-downloads/.gitignore` while leaving `fixed-source` empty. The executed
module SHA256 is
`8e2879c217b88e3d65a449765e0817a07a259c97c778186b2e2309627266127e`,
matching the failed Actions artifact's authenticated `source-files.json` entry.
This is execution of the external module, not an implementation of its behavior
in the regression fixture.

The native regression first failed for Xiaoge with no cache argument where
`/owned/output/downloads` was required. After moving the flag into common
arguments, all six focused OpenCV tests passed. The added negative controls
still reject both a new `.cache/.gitignore` and an edited existing source file
before writing a success receipt. Strict all-target Clippy passed for
`openpilot-dev-tools`. Builds used the coordinated canonical cache with
incremental compilation disabled, two jobs, and more than 378 GiB free before
each operation.

Ignored Actions logs, the original 24-file artifact, module reproduction script
and logs are under `.analysis/scratch/2026-10-14-opencv-cache-fix/`.
The module reproduction runs CMake initialization only; it does not rebuild
OpenCV libraries or establish target/device behavior. Full architecture builds
remain the subsequent CI gate.
