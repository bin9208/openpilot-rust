# Native Panda runtime conversion (#175)

Issue [#175](https://github.com/bin9208/openpilot-rust/issues/175) is part of the
approved full-runtime conversion [#1](https://github.com/bin9208/openpilot-rust/issues/1).
The component is in progress. No physical Panda, USB/SPI device, C3X, vehicle CAN
or firmware-flashing operation has been used. Registered daemon selection remains
unchanged; the following library checks do not establish a complete Panda daemon
or the whole-runtime startup/log-upload gate.

## CAN codec and SPI alert state

`rust/crates/pandad` contains a safe Rust CAN encoder/decoder and the source
`PandaSpiAlertTracker` state. The encoder retains the packed six-byte header,
all sixteen legal CAN/CAN-FD lengths, address/extended bit semantics, per-Panda
four-bus selection, XOR checksum and the 256-byte soft batching boundary. The
decoder retains partial input, rejected/returned bus offsets, already decoded
frames before a later checksum error, clearing the rest of a failed batch and
the caller's required communications-reset decision. Invalid outgoing lengths
produce a fatal typed error where the original asserts; they are not padded or
sent. Irrelevant-bus packets are skipped before payload validation, as in source.

The SPI alert tracker preserves five-second onroad arming, the ten-second
recovered-error window, three-event threshold, one-second confirmation and the
once-per-drive capture latch. Its unsigned time arithmetic matches the source,
including explicit wraparound cases. No scheduling, safety, retry or error
threshold is changed.

## Original-source comparison

`build_pandad_protocol_source.py` compiles the unchanged original `panda.cc` and
`spi_alert.h` with the full original generated cereal C++ schemas. The test-only
facade exposes protected/private state without rewriting the methods. It
substitutes only a transport reset recorder and logging sink; actual transport
semantics remain subsequent work. Source and generated-schema inputs are hashed.

`check_pandad_protocol.py` compares every byte of every output chunk, every
decoded frame, remaining partial bytes, reset/log effects and complete alert
state. Independent fixture checks require all 1,536 intended valid receive-flag
cases to decode one frame without checksum failure. The final corpus contains:

- 664 outgoing batch cases covering all legal lengths, all 256 source buses,
  address boundaries, large batches, multiple bus offsets and seeded input.
- 2,380 receive cases covering all bus/rejected/returned flags, fragmentation,
  all checksum-byte values, corrupt batches, subsequent recovery and seeded input.
- 130 alert sequences with 30,720 state transitions, threshold boundaries,
  repeated captures, ignition changes and unsigned time wraparound.
- Seventeen invalid outgoing lengths. Original processes exit with SIGABRT;
  Rust exits with a typed fatal error. Neither emits a result. The same lengths
  on an unrelated bus are ignored by both implementations.

All comparisons passed on x86_64 and with the aarch64 Rust executable under
QEMU. The original-source executable also passed the same comparison with
AddressSanitizer, UndefinedBehaviorSanitizer and leak detection enabled. Strict
all-target Clippy, Ruff, formatting and diff checks passed. This is host/emulated
evidence, not AGNOS, physical transport, drive or performance acceptance.

Local evidence base: `.analysis/scratch/2026-10-01-rust-pandad/`.

| Artifact | Result |
| --- | --- |
| `source-protocol-2/manifest.json` | Unchanged original C++ build and provenance |
| `protocol-flags-final/report.json` | Exact final host corpus |
| `source-protocol-asan/manifest.json`, `protocol-asan/report.json` | Sanitized original-source corpus |
| `protocol-arm/report.json`, `protocol-arm-build.log` | Exact emulated ARM64 corpus and build |
| `protocol-clippy.log` | Strict native package lint pass |

Host native example SHA256:
`ac28c03f739b79a016c568c09997b598d1bb151a013b97d951d31b1ead01c539`.
ARM64 example SHA256:
`5d6919c145307ea298285d3db72fa182f04115ed6bd78ae0c2de9848fbeb4aec`.
The checker takes `--source SOURCE_BINARY --binary RUST_BINARY --output FRESH_DIR`;
use `--qemu EXECUTABLE --sysroot DIRECTORY` for the emulated ARM64 lane. The
source builder takes existing Cap'n Proto and json11 prefixes; it installs no
dependencies. Invalid-input children disable core-file generation while retaining
their exit status and stderr.

USB/SPI transport, safety configuration, state/peripheral/CAN worker loops,
firmware/DFU supervision, runtime logging and lifecycle composition remain in
progress. Firmware artifacts, libusb and the Linux driver interfaces will remain
explicit external dependencies; the final wrapper will not invoke Python.

Docs-Not-Needed: native library and host validation only; no user setting or
production process behavior change.
