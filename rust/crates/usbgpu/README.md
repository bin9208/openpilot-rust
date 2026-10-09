# Native USB GPU runtime (in progress)

This crate ports project USB GPU policy and tinygrad's USB/AMD host runtime to
Rust. It is an intermediate foundation, not a complete runtime or device-test
candidate. See `docs/naver/rust-usbgpu-154.md` for responsibilities and evidence.
The native GPU probe now has a bounded source/emulator comparison; its real
device execution and target asset packaging remain unvalidated.

Source families are explicit: the repository's tinygrad runtime and the pinned
Cinque v3 artifact's newer HCQ2 runtime have different graph representations.
The preserved implementation includes bridge, allocation, discovery, firmware,
AMD queues, HCQ2 loading/dispatch and worker/client code. Full pinned-model
numerical acceptance is unresolved: the current emulator run produced NaNs and
its source replay timed out. Production does not execute Python. Python files
under `rust/tools/` are developer-only source oracles and metadata generators,
not runtime helpers.

The tinygrad-derived code and generated register/C-layout data retain the MIT
notice in `LICENSE.tinygrad`. `assets/amd-metadata.json` records each original
source path and SHA-256. Regenerate/verify it with
`rust/tools/generate_usbgpu_amd_metadata.py`; this reads declarative syntax only.
The original generator records ROCm kernel-driver and linux-firmware revisions
in `tinygrad_repo/tinygrad/runtime/autogen/am/__init__.py`.

Firmware remains an external native dependency. Every loaded blob must match
the original pinned SHA-256 table. Host validation blobs are private cache files
and are not bundled or installed by this crate checkpoint. The libusb ABI header
has separate LGPL terms and provenance in `native/README.md`.

The inherited stock-UAS stale result-window bug is intentionally corrected;
see `docs/naver/rust-usbgpu-uas-160.md`. All other stated comparisons are bounded
to their recorded scenarios and do not establish device timing or performance.
