# Desire probe output order under workspace features (#463)

Full PR run [39617311670](https://github.com/bin9208/openpilot-rust/actions/runs/39617311670)
at head `f5c8ee8de76c038883f65668a45c49ad559ca83b` failed the workspace Desire
CLI raw-output assertion. Other runtime and ARM jobs passed; the full aggregate
remains failed. The 24,585-row output had the expected 89,830,766-byte length
but SHA256 `195f3f9175b0ea34555fafd0f9ca4bd34e63393a62c05b5518943655eebaf8ae`
instead of the accepted
`71e098692d451676bf1763121a4337ff50c9a4e82c3476c4db0a1422016c2782`.

The workspace example build unifies `serde_json/preserve_order`, enabled by
Athena. The isolated probe used sorted map keys; the unified build emitted
insertion and struct-field key order. Transforming only keys in the accepted
stored rows, retaining every numeric token and value, reproduced the exact
failed SHA. Request and original-reference seals were unchanged. This is a
probe serialization defect, not evidence of changed Desire state/numerics.

The fix recursively sorts the observation's object keys before serialization.
Expected hashes, original source pins, state logic and acceptance policy stay
unchanged. The focused hosted recipe requires a one-frame exact old-source
failure under `preserve_order`, then fixed canonical bytes and the existing
full CLI admission/raw-path check once. No original recapture or unrelated
runtime/ARM rerun is required for this leaf. Completed raw-path evidence is
archived under an ASCII name after comparison so upload retains its files.

Source/format/diff and recipe syntax review passed. Hosted regression and
affected validation are pending; keep [#463](https://github.com/bin9208/openpilot-rust/issues/463)
open until the results are reviewed. No driving or device acceptance follows.
