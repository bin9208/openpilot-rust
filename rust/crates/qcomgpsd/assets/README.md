# Qualcomm GPS report declarations

`reports.rs` is the maintained native wire layout and decoder declaration for
the eight GPS/GLONASS/OEMDRE report types. Integer widths, array lengths, field
order, serialized sizes and float interpretation retain the source protocol.

Provenance: `openpilot/system/qcomgpsd/structs.py` at
`73a9e01709245de97ffb18b1a079b019bc3f9fd9`, emitted by the existing Rust build
script. Only Rust formatting changed during extraction. Original repository
MIT license and copyright apply. Builds no longer parse the Python declarations.
