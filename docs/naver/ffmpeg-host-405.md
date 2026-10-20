# FFmpeg 8.0.1 host compatibility (#405)

- Issue: https://github.com/bin9208/openpilot-rust/issues/405
- Branch: `codex/feat-404-self-hosted-actions`, derived from dev-md.
- Purpose: run the existing Rust media consumers on the user's Ubuntu 26.04
  self-hosted runner with its FFmpeg 8.0.1 repository packages.
- No dev/main merge, release publication or device deployment is part of this change.

## Provider selection

`tools/ci/install_host_ffmpeg.sh` selects an exact upstream patch version
with Ubuntu's package revision allowed:

| Host | APT version | libavcodec | libavformat | libavutil | libswscale |
| --- | --- | --- | --- | --- | --- |
| Ubuntu 24.04 | 7:6.1.1-* | 60.31.102 | 60.16.100 | 58.29.100 | 7.5.100 |
| Ubuntu 26.04 | 7:8.0.1-* | 62.11.100 | 62.3.100 | 60.8.100 | 9.1.100 |

It installs FFmpeg and all four development packages, then checks every
pkg-config ABI version. Unsupported hosts and unexpected ABI versions fail.
`--simulate` resolves packages without installation; `--verify` checks the
installed provider without changing it. Installation requires at least 35 GiB
free; the dedicated build reserves 25 GiB plus 10 GiB estimated growth before
starting installations.

The webcam and WebRTC native workflows and the dev-md host-tool dependency
step use this helper. Their source comparisons remain required. The old
libvpx SONAME package name is removed from package inventory, because codec
availability is verified using the FFmpeg encoder list instead.

The source-built FFmpeg 6.1.1 used by generic ARM/AGNOS builders, its checksums,
licensing records and runtime closure remain pinned. FFmpeg 7.1 used by the
separate encoder staging path is unchanged. This host migration does not
replace vehicle providers or claim an installed FFmpeg 8 vehicle runtime.

## Consumer review and verification

The existing consumers already pin ffmpeg-next 8.1.0. Review of direct FFI
and wrapper uses in loggerd, encoderd, WebRTC and webcam found no required
Rust API changes: the wrapper handles the FFmpeg 6/8 layout differences.
Actual build/test results, rather than this source review, determine support.

`.github/workflows/ffmpeg-host.yml` installs the actual selected provider,
runs logger and encoder tests and native binary/example builds, checks native
WebRTC tests and builds, then compiles pinned OpenCV 4.13.0 and native webcam
against that provider. It records source SHA, package versions, ABI versions
and remaining disk space. Probe-specific build directories are removed at exit.
All functional checks run in Actions on bin-actions; local checks are limited
to YAML/shell syntax, Python syntax and diff review.

FFmpeg 8.0.1 ABI definitions:
- https://github.com/FFmpeg/FFmpeg/tree/n8.0.1/libavcodec
- https://github.com/FFmpeg/FFmpeg/tree/n8.0.1/libavformat
- https://github.com/FFmpeg/FFmpeg/blob/n8.0.1/libavutil/version.h
- https://github.com/FFmpeg/FFmpeg/tree/n8.0.1/libswscale

## Evidence

- Provider/build source: `07cac11af42644be6e0470ca0bbe7b1d6e1fe07d`.
- [Native FFmpeg host run](https://github.com/bin9208/openpilot-rust/actions/runs/39404043064):
  **success** on bin-actions / Ubuntu 26.04.1.
- Installed FFmpeg: `7:8.0.1-3ubuntu2`; all four expected ABI versions matched.
- Existing tests passed: logger 12, encoder 9, WebRTC 5, webcam 4: **30 passed,
  zero failed**. Native binary/example builds passed for each consumer.
- Pinned OpenCV 4.13.0 built against the host FFmpeg 8.0.1 provider; native
  webcam then compiled and linked successfully.
- A follow-up commit `04cea4ab` only updates the webcam CI-policy assertion to
  require the helper's full ABI verification rather than a hard-coded
  libswscale command. No consumer/provider implementation changed after the
  successful native run.
Passing host media tests does not establish full source-oracle equivalence,
ARM/AGNOS ABI compatibility or real-device operation.

- [Fast checks on 04cea4ab](https://github.com/bin9208/openpilot-rust/actions/runs/39404145817):
  success, including updated CI-policy assertions.
- [Updated readiness/package probe](https://github.com/bin9208/openpilot-rust/actions/runs/39404043084):
  success on the provider/build source; includes all 41 runtime-core integration
  tests, working package resolution and about 35 GiB free after probe cleanup.
- Ubuntu 24.04 and device-provider behavior is preserved in configuration, but
  this task did not rerun separate ARM/AGNOS or complete source-oracle suites.
