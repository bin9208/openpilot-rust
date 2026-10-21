# Independent synthetic driving/model observations

Historical source: `e25a14386582858dae25c6daca5ced3bfb4bacab`,
[run39635596188](https://github.com/bin9208/openpilot-rust/actions/runs/39635596188),
artifact12365522894. This package preserves the complete original fixture ledger,
not native-derived expectations. It contains synthetic images and model data;
no vehicle log or credentials are included. Preserve adjacent source licenses.

`fixture.json` is310,291 bytes, SHA256
`afe1c795d7f609801eb4cd5a5a7748c3c996929403a3c0fa102f232f2a8d248d`.
Its1649 files total557,818,481 bytes. The1564 regular USTAR members contain
this unchanged ledger and all ledger entries except86 synthetic NV12 images.
The native stager regenerates those311,042,048 image bytes with the original
PCG64 seed/consumption order, then checks each complete original SHA256 and size.
No expected model packet or report is generated from the native implementation.

`observations.tar.xz` is59,663,776 bytes, SHA256
`3ddfb1213cccbbf0d4d0ef7854b79cef5637acdfd7b59d883b13063d6968b4fe`.
XZ uses a256MiB dictionary; the native decoder limits memory to320MiB.
[Packaging run39649706375](https://github.com/bin9208/openpilot-rust/actions/runs/39649706375)
artifact12368239243 preserves the exact member pins, omitted image roster,
original manifests and selected historical ZIP CRC receipts. The parent checked
all three compact ZIP CRCs and every1564 tar member against the external ledger.
This does not claim all28,880 historical artifact members were downloaded.

The archive contains historical CPU object code and five captured-model PKLs.
**Do not execute the historical objects or captured JIT on an arbitrary CPU.**
Driving frozen comparisons re-export stored program IR with the current host
compiler to a separate catalog, binding CPU flags, LLVM library/package, source,
input and output hashes. The native gate rejects missing/foreign receipts and
rechecks these inputs around both roots. Fresh original driver JIT captures need
current-host model compilation; catalog export alone cannot replace saved JIT
machine code. The current-host compiler/exporter remains pending Python scope.

The stager enforces bounded expansion, exact regular-member/hash roster, safe
paths, no links or duplicates, disk reserve, unchanged input and atomic publication
without replacing foreign output. Its hosted full regeneration/adversarial gate
and four-source driving controller retirement are pending; packaging success is
not functional model, device or complete-project acceptance.
