# Native Panda orchestration retirement (#296)

The native outer controller executes all38 original build/source/transport/
supervisor phases with the same held30 child executables. It replaces only
`rust/tools/check_pandad_ci.py` and `rust/tools/tests/test_pandad_ci.py`. The
remaining Python phase checkers, IPC binding build, Python ABI/usb1 discovery and
source probes remain explicit dependencies. This is not an interpreter-free
Panda test family or a physical USB/SPI/device test.

Independent original/native full38 evidence at cdfdb291 was accepted in
run39557857293, artifact12327083506 ZIP SHA256
`1f64e4436b1af7d000610ea40ab2dff1b4c327bfc7b7287ace3698a6f7abba48`.
The original151-path roster remains immutable. New explicit source additions
are python_fs.rs and pandad/Cargo.toml. Source/raw-path defects452/454/458 are
tracked separately rather than treating the old first-child failure as current.

## Accepted pre-deletion native preparation

[39616860491/job119280134322](https://github.com/bin9208/openpilot-rust/actions/runs/39616860491/job/119280134322)
passed source `1f7a182b0b9708340c5f16b5ed55d6c470f5ad8b`, workflow
`adfd94d6a5278b927b99f427063aeaacfa6dd833`, Rust1.94. The Rust test prepares
a fresh private source tree, excluding only the two retired sources, .git and
build/scratch caches. It reserves and enforces3GiB including filesystem entry
overhead, preserves symlinks/modes and writes independently source-derived native
build metadata before running either lane. No Python private copier or metadata
generator is used. All31 held ELF hashes and source/metadata/build-message seals
are checked before/after both lanes.

Parent audited artifact12352894540 (83,003,018bytes/15,762members, all CRCs), ZIP
SHA256 `e3eb33893aef9aed9d21e0d42e4ddcda3a8fdff02a271bedd02823a6c9d70d87`.
Checkout/private authored source counts153/152 differ only by the historical
checker; every remaining source hash agrees. Both lanes retain all38 ordered
phase passes,42 disk reservations,152 command/exit/stream files and final PASS,
with identical held executable/source inputs. Eleven independently sealed
original CLI cases preserve exit codes, output channels, disk reservations and
observed output ownership/repeated-option side effects. Help wording is native;
no claim of byte-identical Python traceback/help text is made.

Raw artifact12353218654 (24,901,707bytes/14CRC-checked members), ZIP SHA256
`acf9799af5a541e5e12dfded46a09678c23c8c6ee47275c7cd1128ec292d053e`,
records both original/native current controllers passing eight phases through
check-device, then failing build-state with exit1 and no final result. The
builder strictly decodes captured compiler streams, retaining its original
subprocess(text=True) boundary. This is current partial raw-path observation,
not full38-phase raw success or evidence of a new native state-policy defect.

## Accepted actual deletion integration

The two Python sources are now removed and the maintained Panda job builds the
native controller, runs its14 unit cases and strict Clippy, then requires both
source-absent38-phase lanes and sealed original CLI cases. Padding/device
consumers use checkout-output artifacts produced by that gate; there is no
redundant third orchestration. The75-minute job budget covers two full phase
lanes, private copying and the retained padding/device regressions; individual
command, signal and ownership deadlines are unchanged. Source-policy tests
require the native build-before-caller order, actual source absence and all
original downstream checks.

[39625704015/job119310242750](https://github.com/bin9208/openpilot-rust/actions/runs/39625704015/job/119310242750)
passed source249c1a0815e7b43d677065f3bd508b423e5ef6b5, workflow
33f044de284c1d25e5a4ae50f2dbb4e7c983f15d. Parent audited artifact12358686227
(88,463,658bytes/16,108members, all CRCs), ZIP SHA256
`1dffb8ea17d36f68e02b3c59736effbf9667d6225fb13e35eec833f140f321cc`.
Both source-absent roots contain the same152 authored source hashes; parent
matched every hash to that exact Git commit. All31 held executable hashes agree.
Each lane retains38 ordered passes,42 disk reservations,152 command/stream/exit
files, all38 command exits0, empty controller stderr and final PASS. Eleven
original CLI admissions retain matching exits and output channels. The complete
job also passes source policies, controller units/Clippy and all retained
padding/device/downstream checks.

The first actual deletion run39621601211 failed its source policy before this
proof: the reservation total still expected490 after three new native guards.
Commit249c1a08 preserves the historical177-guard list/hash and adds an explicit
three-guard group, requiring the new493 total rather than removing old guards.

Inventory now marks233replaced-local/4partial/3343pending, including these two
accepted source deletions. Keep #296 open. Remaining Python phase checkers and
compiler/tool families are not covered by this retirement. User owns normal
boot, recording, upload and driving checks; no AGNOS/device acceptance is inferred.
