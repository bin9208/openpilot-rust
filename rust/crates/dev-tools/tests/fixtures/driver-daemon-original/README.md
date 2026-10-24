# Independent driver observations

The archive contains three separate, unchanged original observation sets:

| Dataset | Run / artifact | Original manifest SHA256 |
| --- | --- | --- |
| Historical collector | 39633286381 / 12360417493 | `57a3a05c0a0fa5b3318fc3fb1e3c07e9a9838b3379c3cb2e50e96974ed7bfd4a` |
| No collector | 39652828256 / 12369678295 | `1b8e6cea511e9f212cd7737688b3e2ec0a165b9ff985b492f0e98c4639f18380` |
| Current collector | 39663412651 / 12377006222 | `532281d6682b2d5a4ae8b8acf354b3a39d04eaf98cd49cc9a2c1432be30b8795` |

Each manifest seals all 43 original files, 14 independently source-computed Cereal
packets, four cases, explicit source inputs and actual loaded Python sources.
The original checker executes the Rust daemon, but expected packets originate
in unchanged original `get_driverstate_packet`; candidate packet files never
populate the oracle. Logging, queue capacities, raw predictions, scenarios,
frame schedule and numeric comparison policies remain unchanged.

The current collector capture ran source
`2ecf3f6813f601ea52e7a61ccf75219952daefa0`. It reused the exact no-collector run's
four PKLs only after matching LLVM CPU/features, library hash, package versions
and compiler/model dependency sources. Its admission and full current-target
catalog receipt are retained. This was a new independent collector observation,
not a relabeling of the historical collector. Both current modes reference the
same exact model payloads under `no-collector/models`.

The historical collector's model payloads were not archived. Its original model
pins and all observations remain intact, and its execution readiness remains
false. They are not replaced by current model hashes.

`observations.tar.xz` is 8,254,920 bytes, SHA256
`0b4cbeb4b8c6201c150ccc85c916e5ff979b66da433c217a2f198ad4bd8c1210`.
`fixture.json` is 45,056 bytes, SHA256
`c55233339671d092fc7d67485e59c639b6fab1ffb03b1c4ece5138e4e9547a33`.
The archive packages 97 data members and omits 42 synthetic NV12 images. Native
staging regenerates zero warmups and NumPy PCG64(seed 20260930) full-range uint8
images, checking every original SHA256. Complete staging yields 139 files and
168,356,431 bytes, bounded by 192 MiB plus the standard 25 GiB disk reserve.
Extraction rejects links, escaped paths, duplicate/extra/missing members,
nonzero tar tails and trailing/concatenated compressed streams. Publication
uses an exclusive atomic destination and preserves foreign outputs.

Data readiness is true; native retirement acceptance is false until the grouped
same-ELF collector/no-collector, source-absence and CLI gates pass. Staging never
imports Python, unpickles models or executes historical machine objects. Future
native execution must export the stored IR for its actual host and hold a
separate authenticated current-target catalog receipt. An x86_64 PKL is not
portable executable code merely because another host is also x86_64.

All data are synthetic; no vehicle captures, credentials or settings are included.
Project provenance/licensing remains under the repository LICENSE; generated
Tinygrad artifacts retain `tinygrad_repo/LICENSE`.
