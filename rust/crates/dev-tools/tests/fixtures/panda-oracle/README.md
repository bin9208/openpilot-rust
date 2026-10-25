# Panda build-tool source oracles

Captured on 2026-10-15 from the unchanged Python tools at repository baseline
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.

`original.json` records 17 successful cases: all five protocol modes with and
without sanitizers, USB/SPI/peripheral with and without sanitizers, and the
runtime libusb fixture. Compiler selection is explicitly exercised for USB/SPI.
A controlled subprocess peer records argv, creates fixed compiler products and
four named generated Cap'n Proto sources, and returns fixed stdout/stderr.
The original tools themselves perform all extraction, hashing and manifest/log
writing. `$ROOT`, `$OUTPUT` and `$DEPS` replace only absolute fixture paths.
The libusb header contains `controlled libusb header\n`; generated C++ source
contents are `controlled generated NAME\n`; compiler products contain
`controlled compiler product\n`.

`original-failures.json` records original artifact retention for compiler failure
in all five builders, plus both protocol command stages and USB's second stage.
The controlled peer returns exit status 9 at the named stage. No real compiler,
network request or hardware access was used to capture these fixtures.

These are independent pre-port outputs, not expectations computed from the Rust
implementation. The integration test compares original argument order, extracted
include bytes, source hashes, command logs, manifest fields and failure artifacts.
Actual C++ compilation and existing source/native policy comparisons provide
separate validation beyond the controlled command contract.

`original-runtime-compiler.json` independently captures the original runtime USB
fixture's explicit `--compiler controlled-clang++` interface. It supplements the
17-case baseline and guards both compiler command selection and CLI help.
