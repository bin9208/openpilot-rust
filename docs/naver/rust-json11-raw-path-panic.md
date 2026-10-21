# JSON11 staging raw-path defect (#452)

Original-only hosted run39554286034 used a valid build-messages file and an output
directory containing byte0xff. The Python Panda controller admitted the path,
printed three disk reservations and retained invocation/command/error receipts.
Its first native child stage-json11 panicked with exit101; the controller failed
with1 and wrote no final result. This is an observed failure contract, not a
successful raw-path staging result.

The child starts parsing with std::env::args() in json11_stage.rs:144, which panics
on a non-UTF-8 Unix argument. A complete repair also needs lossless curl -o argv
at line69 and a surrogate-preserving success receipt at line176. Merely changing
the parser leaves lossy path selection and JSON serialization behind.

Preserve archive hash, path/symlink/size admission and disk checks. A focused
hosted regression should stage the pinned wheel to a real raw-byte directory and
verify downloaded/extracted bytes and receipt path round trips. Also retain
normal staging tests/Clippy. No local functional execution or device test was
performed. No repair is included yet: the current Panda parity proof retains the
original observed exit101, and this child defect needs separate validation.

Original artifact ZIP SHA256:
c3425d3db45b5df9296e1421f9736f0af52ba70aa869bc66452266e2d2cc0657.
This is part of #296 and does not affect dev-md/#338.

Tracked independently in [issue #452](https://github.com/bin9208/openpilot-rust/issues/452); controller retirement remains part of #296.


## Accepted child boundary, caller verification pending

[39563459189/job119099964089](https://github.com/bin9208/openpilot-rust/actions/runs/39563459189/job/119099964089)
passed source `a35d166ebd66362effca8c1e832887b6889ec4dd`, workflow
`490e0c3537e4dd64dd1456868dbbabd8d72ad301`, selected Rust1.94.0. The new
raw-download test failed on the exact old stager as required, then the fixed unit,
all seven native-build-closure tests, strict Clippy and actual locked-wheel
staging passed. The first diagnostic failed before its negative control because
the old source was not fetched; another used runner-default Clippy1.99 and
stopped on unchanged USB lints. Neither is a demonstrated child regression.

Artifact12329146259 ZIP SHA256 is
`bdcaaaa19a628f589ee1ac56cef9975623a66a22c2ba7b2d9b181e1df2d9e86f`
(11members, CRC checked by parent). The parent separately read the raw-name tar,
verified wheel SHA256 `a309d401dd22b758455dafd3c28b72c4bd88c7482ddd70fe8653e53efc745a83`
against the locked wheel, compared all seven extracted member bytes, and checked
the0xff output/prefix bytes in the JSON receipt. Archive integrity, size,
architecture, symlink/path and disk admission are retained.

The Panda caller formerly reparsed the success prefix using serde_json String,
which cannot retain a lone surrogateescape byte. Commit45e797e3 preserves that
filesystem path with JsonValue/codepoints and checks its lossless round trip.
A focused native case requires reaching the next build-protocol command receipt;
fresh original/native raw observation, eleven CLI cases and strict Clippy are
queued separately. Do not reuse the old first-stage panic101 as the current
controller outcome after the child repair. Keep #452 open until that current
caller evidence is inspected. No full Panda raw success or device result is claimed.


## Current caller and unified-feature evidence (2026-10-21)

The current original/native Panda raw observation at source312f4827 in
[39568840592/job119117661612](https://github.com/bin9208/openpilot-rust/actions/runs/39568840592/job/119117661612)
reaches stage-json11, build-protocol, check-protocol and build-safety successfully
on both sides, holding the same30 ELFs. Both then fail check-safety on its separate
raw Params JSON boundary (#458). This supersedes the old first-stage101 as the
current controller result; it is not full38-phase raw success. Artifact12332475688
ZIP SHA256 `924ef4c54df0299cc923fc41d8fbaf9de83f3c38e16f27bab8f02bf86d10c2cf`
has14 parent-CRC-checked members and an independently inspected raw-name tar.

Whole PR39567841179 subsequently exposed normal receipt byte-order divergence
when another workspace crate enables serde_json/preserve_order. Commit100b004d
constructs the receipt in the same map order as the original json! value while
inserting the lossless raw path. The byte-equality assertion is retained. Hosted
[39614534323](https://github.com/bin9208/openpilot-rust/actions/runs/39614534323)
passed source `2c004e0e7dccc4258b13e436b2d2961ced74ddca`, workflow
`056b7b87839d740d53ace40864ab77a017e793a0`: old-source unified-feature RED,
fixed default test, all83 library tests with preserve_order, both strict Clippy
gates, seven closure tests and actual locked-wheel raw staging.

Parent checked artifact12350954796 (98,156bytes/14members, all CRCs), ZIP
SHA256 `60c9c95e19977cc69e9a1a3df7eb340467b0e5a7f2b965855d6f6854bf8d4675`.
All seven extracted members equal the independently read locked wheel, whose
SHA256 remains a309d401dd22b758455dafd3c28b72c4bd88c7482ddd70fe8653e53efc745a83.
The archived raw0xff root and absolute receipt prefix bytes match. The bounded
JSON11 child/caller defect is verified; Panda retirement and later phase defects
remain separate #296/#458 work.
