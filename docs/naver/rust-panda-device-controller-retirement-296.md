# Panda device comparison controller retirement candidate (#296)

This candidate replaces the comparison controller and its generated case corpus,
`check_pandad_device.py` and `pandad_device_cases.py`. Their removal and maintained
native reference caller are now integrated as a deleted-checkout candidate.
Deleted-checkout validation is accepted in run39640526320 at c056e379. The two
sources are marked replaced-local. This establishes controller retirement, not
physical Panda/USB/device behavior.

The original-only capture executes the unchanged Python checker against the
original C++ device probe and current Rust device example. It retains all1,746
requests, 2,490 operations, both responses, parser admissions and failure effects.
Only the C++ `source.jsonl` is an expected-response provider. Captured native
responses cannot populate or repair expected answers. The original C++ ELF is
borrowed from accepted run39616860491/artifact12352894540, with its immutable ZIP,
ELF and14 build-source hashes checked against the current checkout. This is
explicit reuse of that source-matched probe, not a newly rebuilt C++ claim.

The native retirement test requires the complete consumed-file roster in an
externally pinned original capture manifest. It rejects symlink files/directories,
missing seals, changed source inputs and changed executable identities, then
revalidates those seals before acceptance. It compares all request and response
fields and case order, retaining only the original normalization of unspecified
health `interrupt_load`. The separately recorded `interrupt_bits` remains checked.

Actual native CLI lanes cover checkout, private source absence and a raw0xff
output path. The checkout is present before deletion and must be absent after
deletion. Eleven
parser admissions check independently observed exits and output channels, no
false PASS or filesystem effects. Missing source/native probes additionally
check source-first failure order and exact retained files. A preexisting foreign
report must remain byte-for-byte unchanged. Help/error wording is native; these
checks do not claim identical Python traceback text.

The controller admits the two historical files only together as regular files
with the original hashes, or together absent. Mixed presence, dangling links,
other file types and mid-run reappearance reject before final success receipts.
Explicit `predelete`/`postdelete` test modes distinguish present-source validation
from required actual absence. The final deletion gate must not restore, download
or execute the retired Python files.

Candidate39629276623 passes all10 controls and3 full retirement tests, then fails
because its recipe omitted the Clippy component. Corrected
[39631590479](https://github.com/bin9208/openpilot-rust/actions/runs/39631590479)
passes the functional checks and strict Clippy at source
`7fc4698e3d4564a6bd8499ec98a80704f3cd9c77`. Parent audited original artifact
`12359646845` (ZIP SHA256
`b454a1c91ec7c0856f56b5e5a0216f5d4e366154c162c1540bcd35d529ba6717`),
its complete 33-file sealed roster and all 14 C++ plus two original source pins.
Original manifest SHA256 is
`ff7b197b3eca5408abbf7dee5b8df196a8fdd5ea61b4e394e829ca382428ea8d`.
Native artifact `12359402144` has 38 CRC-checked members, ZIP SHA256
`8e95eb5692736118c9a4041b36319b1c88d2d46d7d237b1ae78b8f661939f40f`.
This unchanged original observation corpus will be reused for the remaining
caller/deletion integration rather than recaptured per file. Parent also found that the
artifact uploader omitted the raw-byte directory although its live assertions
passed. The next proof retains actual argv/status/streams for every native CLI
call and relocates completed raw-path evidence to an ASCII archive directory,
with hashes checked before/after and a separate explicit archive mapping. Those
evidence additions still require hosted validation.

The two device Python files are removed and their hosted deletion gate is
independently reviewed below. The separate outer
Panda controller source deletion and its38-phase gate are accepted in
[39625704015](https://github.com/bin9208/openpilot-rust/actions/runs/39625704015);
that evidence does not retire this separate device-family controller.

The next family integration routes device, protocol and CAN-I/O phases directly
to their native executables and records each executed source root. Their three
new executable seals extend the outer held set from 31 to 34. The historical
151-source fixture stays immutable; explicit current native additions give a
164-source candidate roster before this family's source deletions, 162 afterward.

`stage-pandad-device-reference` stages a repository-owned compressed synthetic
fixture, replacing the expiring CI observation download for maintained checks.
Parent compared all 35 packaged files byte-for-byte with the accepted original
ZIP: 33 sealed observations, the capture manifest and historical builder JSON.
The 2,333,895-byte blob expands to 14,321,241 bytes. No original Python code or
ELF is packaged. Its native materializer and adversarial controls pass in the
deleted-checkout gate below. Historical builder and current build receipts remain separate;
the final reference gate must validate the actual executed C++ ELF and all 14
current build inputs without rewriting the historical observations.

## Accepted deleted-checkout gate

[39640526320](https://github.com/bin9208/openpilot-rust/actions/runs/39640526320)
passes at source `c056e379d5346f599655864f8b6adcab5d0674f8`, workflow
`c4b3fbaacfc82b47b78cf2ee0b3f8454611358c9`. It runs ten controller controls,
native staging/adversarial integration, all three maintained reference tests,
the archive-extraction unit and strict Clippy. No retired Python file is restored,
downloaded or executed.

Parent audited every ZIP CRC: native artifact12364228304 has 67 members and
SHA256 `1861df2db9629b69336bef5800c3e71aad1438a53e324ca234d3e2afcd0a933f`;
staged-original artifact12364807727 has 44 members and SHA256
`08103534a782a9879dc73b738146ae17d49d2d1e3000ae1869802e9872fe370e`.
The original manifest, its complete 33-file roster and all14 C++ source pins
remain unchanged. Historical and current borrowed build receipts agree with the
actual executed ELF; the gate explicitly identifies reuse rather than a fresh build.

Checkout, private-absent and actual raw0xff CLI lanes each retain all1,746 requests,
2,490 operations and byte-exact source/native JSONL and stderr. Parent checked
the actual argv, exit, stdout, explicit environment, source absence and report
identities in each lane. Eleven parser admissions and three failure effects
match original status/output channels; the foreign report is unchanged. The
raw-path archive mapping retains all seven files with matching SHA256 values
and links them to the raw0xff argv receipt. These fix the earlier uploader gap.

Remaining Panda protocol/CAN-I/O/shared-corpus retirement and the updated outer
integration are separate gates. Device boot, recording and upload are user-owned.
