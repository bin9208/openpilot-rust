# Native CAN policies

This crate ports the current `opendbc/can/{dbc,packer,parser}.py` policies and
their checksum callbacks in Rust. Source-derived data/provenance and original
licensing remain explicit. It contains no Python runtime or project callback.

`Dbc` loads the shipped/generated DBC format. `Packer` preserves ordered values,
source bit layouts and source counter/checksum behavior. `Parser` preserves
CAN packet history, source counter recovery, timeout/registration grace,
lazy registration and accepted raw payloads. It uses arbitrary-width integer
values to retain the source's 64-bit packing/conversion behavior.

The public vehicle controllers, fingerprint/firmware startup and continuous
card daemon are separate, still pending work. This crate alone supplies no
complete vehicle interface or runtime handoff. Existing source failures are
preserved and separately tracked: PSA configuration/missing DBC (#156/#180),
MLB checksum call (#178), and the FCA vector/test gap (#181).

Validation helpers are `rust/tools/check_can.py` and
`rust/tools/check_can_checksums.py`, using `can_trace`/`checksum_trace` examples.
They run unchanged source at test time; native binaries do not invoke it.
See [the component record](../../../docs/naver/rust-card-177.md).
