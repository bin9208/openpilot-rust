# Panda protocol/CAN-I/O controller retirement (#296, #464, #465)

The two comparison controllers and their shared Python case generator are removed
at `83f1da3f49ecffe9037c97476ae188ddcee9a562`. Their actual callers use Rust.
Native maintained fixtures preserve independent C++ observations; no retired
Python code is restored, downloaded or executed by the deletion gate.

[39647358174](https://github.com/bin9208/openpilot-rust/actions/runs/39647358174)
passes at that source, workflow `15653a353f3179016cd32ed25765696b3c565ee9`.
It runs34 integration tests:14 outer mock controls,5 controls and4 references
for each controller, and both native fixture adversarial integrations. The
required-first parser regression, both archive-extraction units, explicit runner
separator rejection and strict Clippy also pass. The14 outer controls are mocks;
the updated actual38-phase/two-root outer integration remains a final CI gate.

Parent and independent reviewer checked all625 ZIP CRCs in artifact12367883326,
45,140,747 bytes, SHA256
`6066f7715e54cd3d484adee96e8bc051baac03727a55ec1065dac516488ffbc5`.
The full comparisons cover checkout/private absence/raw native paths and actual
raw runner bytes, all requests/responses/case order, failure effects and original
parser admissions. Protocol retains3,174 cases,30,720 alert steps and1,536 flags;
CAN-I/O retains normal/MAXOUT runs with1,085/1,082 messages and3,857 operations.
No codec tolerance, bitfield, transport cadence or safety behavior changes.

The protocol fixture preserves all317 original observations, capture manifest
SHA256 `bb63c7f4cf0a39971ae8b2306294179387fad1474b5ec0e0cce5c298de486902`.
CAN-I/O preserves76 observation files and reconstructs the original78-file seal
roster with two explicit ELF omissions; neither omitted historical ELF is run.
Its original manifest is `23534076aec940b91d282c8298609d4b5db46974ab394d0ea05a7c4f3be81531`,
and explicit adapter manifest is
`4a4f41552c597d39dcd45ba18b77ff64a1fd584ddcf63981c7523a5ecf4dbd83`.
All29 admissions and12 fresh original parser probes remain sealed. The current
raw runner comes from the current system and has separate actual/historical
hash receipts. Both14-pin C++ source rosters match Git and the independently
executed borrowed C++ ELFs; historical and current build receipts stay separate.

Earlier combined runs failed because a bare runner separator was wrongly
admitted (#464), then because ordinary JSON metadata compared serialization
order rather than values (#465). The fixes preserve original required-option
precedence and attached-runner behavior. Only ordinary metadata uses structural
JSON equality; wire bytes, raw commands and executable identities stay strict.
Completed original captures were reused; no original corpus was repeated merely
to repair a metadata assertion. Frozen native outputs are additional regression
data, never the independent expected-response provider.

Main CI stages the repository-owned data natively and binds fresh C++ receipts.
The immutable historical151-source roster is retained; current native additions
and source removals yield163 outer sources and34 held executables. That updated
actual outer gate and final required integration remain pending. These accepted
controller deletions do not close remaining Panda/control/compiler/tool/vendor
conversion or user boot/recording/upload/driving acceptance.
