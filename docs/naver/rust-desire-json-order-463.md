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

Accepted focused regression:
[39639003843](https://github.com/bin9208/openpilot-rust/actions/runs/39639003843)
passed at source `95d124cd3ae342459c30f56f4334621507e2be07`, diagnostic workflow
`cc348e1f84609964255aa0272fe1b0d12bf9261a`. The exact old-source one-frame
control rejects the canonical hash; the fixed first row has accepted SHA256
`053b0e2fccfc194f2b1adb17be3223d2993d72e2ebdde8cfad47f48df24f32ba`.
Full CLI admission/raw-path checks and both strict Clippy gates pass with
`preserve_order`. All 24,585 rows match the accepted 89,830,766-byte wire hash
above, with zero maximum numeric error and unchanged `1e-10` tolerance.

Parent audited artifact `12364386736`, 64 ZIP members, SHA256
`c73814fe3b49eef434bcd529b05de2968e3bf7fc01796bb493a630fd2536223d`:
all CRCs, complete request/reference seals, all 28 historical source hashes
against Git source95d124cd, unchanged 31 held inputs, fixed probe source bytes
and both full/first-row hashes. The three additional final bindings are the
expected stdout/stderr/actual output files. Actual raw0xff output was archived
with its directory inode unchanged. This closes the bounded probe defect
[#463](https://github.com/bin9208/openpilot-rust/issues/463); the earlier whole
PR remains failed and current integration is separate. No device claim follows.
