# Independent original firmware SPI fixture builder evidence

The unchanged Python driver has SHA-256
`138b4610e768c5f55b0ee9406500bf4987ee1d88be552a42e9cb07fe5b110c88`.
The 26 independently executed original cases preserve both compiler commands,
separate UTF-8/universal-newline logs, NUL, decode/spawn/nonzero/signal failures,
late provenance failures, lexical and resolved paths, CLI forms and exit codes.
The native tool receives the current interpreter's include path and extension
suffix explicitly; it does not invoke Python to discover or compile them.
The 512 MiB growth reserve is an added admission rule required by repository
policy, before creating output or invoking compilers.

Fresh original/native shared libraries matched byte-for-byte:

- Syscall fixture: `6c79a5eb54544d5d7ed21fb5e6e9513b82e03ff8fc038ec46c1948f89012fdd4`.
- CPython binding: `6e5e62aa775f2c5366bd05b1577f99677585ab0a9c6b1bcd40bff64335e8cf90`.

The spidev 3.8 source archive was verified against `uv.lock`. Three safe original
Python/C-extension comparisons matched 40 calls with both builds, using a
marker-checked owned preload fixture backed by `/dev/null`. No physical SPI or
vehicle was accessed. Original C-extension handles remain cached until fixture/
process teardown; observed counts 1/2/1 match both builds. The separate native
Rust cleanup requirement of zero handles remains unchanged.

Evidence is indexed in private scratch
`.analysis/scratch/2026-10-15-panda-firmware-spi-builder/`. This retires only the
build driver; the original C extension and Python comparison remain pending.
