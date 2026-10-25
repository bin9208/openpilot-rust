# Panda allocator-padding checker retirement evidence

The executed original capture in `edb42bf0-original/` comes from
[`rust-panda-runtime-evidence`, artifact 12289868344](https://github.com/bin9208/openpilot-rust/actions/runs/39477200877),
job 118816129716, source SHA `edb42bf095d6a3e3b38cc0a4546976dfb710363e`.
The 44,204,423-byte downloaded artifact ZIP has SHA-256
`40026fdfe4f4a6dcd804f1e48208a1e41c38ddd9cff025fd83c6cb19da996d01`.
The job's ten controller tests, three live-pair tests and Clippy passed at
2026-10-20 18:57:10 UTC (2026-10-21 03:57:10 KST). The later device failure
is separate (#425).
The artifact held the actual Python controller's `report.json`, process stdout
and stderr, and all three phases' request and subprocess transcripts. These
twelve raw files are copied without normalization. Tests requiring the original
checker copy it from the unchanged source checkout while it remains present;
the capture does not contain a copy of its source code.
`SHA256SUMS` is pinned by
`pandad_safety_padding_check` at SHA-256
`3a12023217195181b9f1d05d06803a4b0add0438baf700abfeb5a7ad0f3f22ea`.

The historical process used source and native Params roots below
`/home/runner/work/_temp/panda-native/padding-controller/pair-vsc2ns/original/`.
The source probe ELF SHA-256 was
`a999145827925c6f4d8c18490d0de4a0f837e9b33f6ce4ec4aa8d32bad2f5636`;
the native probe ELF SHA-256 was
`297614d100b3ed51211e2a64e2cf18a099cb7ecc1a578c8766856fef4a70527f`.
The original report records a 312-byte CarParams message and 304-byte
truncation, with final original safety parameters 4369 and 8738. The source
probe's complete 21-input build map remains in the cloud artifact's
`oracles/safety-src/manifest.json`. The direct admission hashes are narrower
and must never replace that compiled-input map.

`contract.json` retains the six unchanged direct source pins and the former
Python checker digest. Native admission permits two stable states at the
**exact checker path**: absent, or a regular file with that historical digest.
The tools parent and six other pinned files remain required. Symlinks, changed
files and presence changes during the run fail. The original Python file is
intentionally still present in this preparation batch. A private test admission
root omits it, so the next cloud run can compare original Python, native with
the pinned original present, and native with it absent against the same fresh
probe ELFs. The executed fixture also gates native comparison without generating
expected answers from the new controller.

Before deleting the original, CI must prove all three fresh same-ELF lanes
and audit the absent-lane exec trace for interpreter independence. Both native
lanes run with interpreter-free `PATH`. Preserve all raw transcripts and the
source-builder manifest. After deletion, replace the
live-original test with the sealed-fixture gate while retaining the native
process, negative controls and compiled-input manifest check. Local work is
limited to source, format and diff review; no local build or probe was run.


The separate tracked `edb42bf0-source-build-manifest.json` is the actual original
build manifest, SHA-256
`5590942fb8df47efe83cd9d34742439e195e1674a2180bca74cdd291c2e6dbb7`.
The fresh manifest must preserve its full 21 source keys and hashes, with every
current source byte checked. Original/native controller receipts retain complete
JSON comparison; their JSON whitespace and key formatting are Rust-owned.
The two same-ELF native lanes additionally require byte-identical stdout/stderr,
and all probe streams stay byte-identical across all three lanes.

[#430](https://github.com/bin9208/openpilot-rust/issues/430) extends descriptor
retention through `Completed::finish()` after signal-handler teardown. Four
standalone regressions cover late native/source drift, exact-path reappearance
and a foreign receipt. A focused old-edb gate must actually fail its post-execute
mutation assertion before current results can count as fixed.
