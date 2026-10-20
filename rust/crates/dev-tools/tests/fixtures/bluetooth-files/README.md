# Bluetooth atomic-file checker source contract

The original/native gate passed at commit
`1364e48c96e888fa5d178c5eb2132d135cf30765`, Actions
[run 39190411681 / job 117862093373](https://github.com/bin9208/openpilot-rust/actions/runs/39190411681/job/117862093373).
The individual file-controller six tests, strict Clippy and actual CLI with 15 cases
passed. Later unrelated job failures do not invalidate or broaden that evidence.
Retirement-specific source changes and seven tests still await their cloud gate.
The Python checker remains present pending parent-owned deletion/proof.

`original-observation.json` is copied **byte-for-byte** from the independently
produced Python observation in artifact 12159974238, entry
`ui-connectivity/bluetooth-files-original/original-observation.json`.
Its SHA-256 is
`a70cdb6d3bf57a44e0d711149500759895c0af93032b6df922ab5d1917c01273`.
The downloaded archive matched GitHub's digest
`52b48b41f8d2477f3e04b6f10cb0ca49472b86e761533d176cdf43c28e594636`.
`provenance.json` records the job, original source pins, full input hash and length,
and observation identity. No candidate result supplies golden values.

The original test persisted the source hashes, lengths, permissions and outcomes,
not its temporary raw original files. The archive's native-controller `original/`
files were authored by the independent Rust reference encoder; they are **not**
relabelled Python captures. Tests reconstruct expected bytes with that separate
reference encoder and authenticate **every full buffer** against the sealed
actual-original SHA-256, byte length, permission mode and Unicode outcome before
using it. The complete 12,963,708-byte input likewise must match the original
`a548190ff3cfdaf9649197573cd7756d9994c79fe66d9dec455d051bcf69913a`
digest. This compact fixture retains all15 observations without checking in the
4,382,736-byte Unicode file or a copy of retired Python source.

The schedule includes null, both booleans, zero, negative zero, two float exponent
boundaries, infinities, NaN, a nested object with a 201-digit integer and
Unicode/NUL/DEL, three lone-surrogate failures, and every 1,112,064 Unicode scalar.
All 15 full bytes, original modes, directory cardinality and temporary cleanup
remain strict; expected output total is 4,383,050 bytes. The real native candidate
still has the original 30-second subprocess deadline. Existing cancellation,
identity, mutation, raw argument, and publication controls remain mandatory.

`source-pins.json` retains the original checker hash as **historical** provenance.
Only `rust/tools/check_bluetooth_files.py` may be absent. The three-entry source
pin roster is fixed, so removing a required live pin also fails. If the retired
checker is present it must still be the known regular file. Changed, symlinked,
dangling and nonregular entries fail, as does reappearance during a comparison
that admitted absence. The model and
native example source remain required live pins; neither is retired. Reports
separate `historical_source_sha256` from current `input_sha256` identities.

Local preparation used source/data inspection, artifact extraction/hash checks,
formatting and diffs only. No local builds, regressions or original execution
were performed. Parent owns caller/CI migration and exact deletion after the
required cloud and same-ELF absence proof. No device, CPU or allocation claim.
