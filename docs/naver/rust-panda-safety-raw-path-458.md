# Panda safety QA Params filesystem path (#458)

Original/native controller source312f4827 in39568840592 both passed the JSON11,
protocol and safety-build boundaries, then the native safety example rejected
raw0xff in `params_root`. The parent independently audited artifact12332475688,
ZIP SHA256924ef4c54df0299cc923fc41d8fbaf9de83f3c38e16f27bab8f02bf86d10c2cf,
14CRC-checked members: both lanes exit1/fourPASS/five equal exit receipts and no
final result. This is QA path admission, not a safety-state or device failure.

The candidate retains the path's raw JSON token through Serde, decodes Unix
surrogateescape bytes, then opens Params at the exact filesystem path. A direct
request struct prevents internally tagged enum buffering from eagerly decoding
the raw token. Operations, parameter bytes and Safety policy are unchanged.
Two focused tests cover actual raw-root creation, ordinary Unicode, invalid
path types/lone surrogates and unknown operations. The example alone uses the
existing log-format crate and tempfile as development dependencies; the locked
package graph records both. The Panda source fingerprint now includes its
Cargo manifest, alongside the previously added shared filesystem helper.

Local formatting, TOML syntax and diff checks only. Hosted old-example RED,
fixed GREEN/full Panda comparisons and affected ARM evidence are pending.
[Issue458](https://github.com/bin9208/openpilot-rust/issues/458).


Hosted [39616860491/job119280134322](https://github.com/bin9208/openpilot-rust/actions/runs/39616860491/job/119280134322)
passed source1f7a182b0b9708340c5f16b5ed55d6c470f5ad8b/workflow
adfd94d6a5278b927b99f427063aeaacfa6dd833. Exact old example exits1 with
lone-leading-surrogate decode error and creates no root; fixed example exits0
with results[] and creates the actual raw0xff Params root, independently checked
in its GNU tar. Both focused tests and strict Clippy pass. Parent audited
artifact12352849676 (6,960bytes/15members, all CRCs), ZIP SHA256
`102c852332da6f62871a5253f797080d4c0aca46866bd98dfd2d8582fa30a759`.
The full same-source Panda two38 lanes and11 CLI cases also pass. Current raw
controllers advance through safety/padding/device and fail a later strict-text
builder boundary; no full raw38 or physical Panda acceptance is inferred.
