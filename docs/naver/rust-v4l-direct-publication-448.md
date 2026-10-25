# Direct V4L publication into owned message storage (#448)

The #447 implementation reuses a scratch Vec but still copies each packet from DMA into scratch, then into Cap'n Proto. This candidate fills Cap'n Proto's owned `init_data()` storage through `Mapping::copy_into()` and removes the intermediate Vec. It exposes no Rust reference to DMA storage. The original C++ retains the dequeued capture buffer through serialization and requeues after publication; the Rust worker keeps that order, cache synchronization, size and timestamp checks, paired metadata, headers and EOS handling.

Software publication retains its slice interface. The shared message builder preserves clock call order, fields, encode IDs and length-conversion placement. Moving the DMA copy into message construction necessarily samples clocks earlier relative to that copy; it does not preserve wall-clock values across runs. No vehicle timing claim follows from fixed-clock packet equality.

An independent prior `set_data()` field construction checks complete serialized messages at fixed clocks for variable, empty, maximum and shrinking payloads. A second comparison covers all eight services and three codec types. Oversized mapping admission must fail without consuming an encode ID. The warmed benchmark uses identical message construction, clock callbacks, count resets and queue sends for both paths. Its explicit payload-copy byte accounting excludes serialization and queue copies; elapsed values are samples with no speed threshold. Test transports use unique owned queue names.

Parent source review found and repaired shared test queue names, a shifted software failure phase, and unequal work in the first timing harness. Hosted unit and complete encoder/codec/V4L/IPC comparisons, strict Clippy and affected ARM validation remain required. The prior #447 measurement is historical scratch-copy evidence, not a measurement of this direct path. No local functional execution or device test was made.


## Accepted full host proof, 2026-10-21

Sourcea6c3201fe9d6cc228bfa8b54abf60ef0e6d35d82 passed
[run39546722792/job119046445349](https://github.com/bin9208/openpilot-rust/actions/runs/39546722792/job/119046445349),
workflow1667cdf24f63f9c36a31b99b74d33f9f9eace9ad. Its wrapper retains the historical
#447 name; the checkout and actual test sources are #448. Complete wire controls,
full codec/V4L/runtime/IPC comparisons and Clippy passed after fixing only the
missing test namespace (unique runtime queues avoid global environment mutation).

Across80 warmed message constructions and queue sends, selected payload-copy
accounting was205,400→102,700bytes, allocations240→240. The elapsed samples were
880,306/854,076ns and are not a measured speedup. These are the same-builder host
fixture values, not hardware VIDC throughput or vehicle CPU/latency evidence.
The selected accounting excludes serialized-message and queue copies. Affected
ARM verification and user device acceptance remain pending.


Artifact12322940409 ZIP SHA256
`ed9838b0bbc902588128041ca445d3eb908451074921d442d90576fb0a2d1921`
was independently inspected by parent and reviewer:5codec cases/112packet hashes,
32V4L cases and10runtime cases preserve exact bytes and metadata. The8×3service/
codec and variable-packet tests and strict Clippy also passed. No hardware VIDC,
normal startup, recording/upload or device performance result is inferred.

Accepted affected ARM build checkpoint (2026-10-21):
[39552825804/job119065662791](https://github.com/bin9208/openpilot-rust/actions/runs/39552825804/job/119065662791)
succeeded at PR merge `a3a5b0f1960237e9e6b65c5a2893eb5295f73514`, merging
head `11af3760cada240d48c0dec0e3a732fe485a267c`. The full run failed separate
required checks; this is ARM build evidence only and predates cweb450.
Encoder artifact12329423934 ZIP SHA256
`e6d48cc58a67d6bca14630ef0ef33caa446fdf0b1307a4af4d0fb20fc3c57ef7`
contains17,341CRC-checked members. Parent verified AArch64 ELF headers and hashes:
encoderd061c3d25ca740fa1f6988605ef94cd35ea5e01da3586cfd8be1ac438712a2f35,
codec tracec777ca70abeebba3b7ac138d70fb9f4a510bb1ed7d9dd13537f8d689ce465672,
V4L trace6236df4b53b99fbf9d707eb46f3d1a359176d9ff8715086d5deb57e811b73cb2.
Generic ARM artifact12329765174 ZIP SHA256
`10cec19501648d42dfa43cac96e58c99beef68822ec77a1b590f58fcfa440b17`
contains83CRC-checked members. Dashcam upload GNU/musl ELF hashes are
fc94f4497c83404b9d422cd643e296705ead6022448d6858583971cc27784968 /
74cd303566be35d467f9c685491072fe8461d5d54ce96e54e2e3effb3eb5c593.
These are generic ARM build outputs, not sealed AGNOS/device, V4L execution or
ARM performance evidence. That checkpoint is historical. The newer ARM result below supersedes its
pending updated-build status; whole-PR and device results remain separate.


New affected ARM [39567841179/job119114353753](https://github.com/bin9208/openpilot-rust/actions/runs/39567841179/job/119114353753)
passed at merge checkout a9d151ef1bc4486157d40179b3057644938c62b7, merging
head917cd7db into dev b55552db and including #448/#449/#450. The full PR failed
separate QA/workspace checks. Parent audited all17,341 encoder artifact12335387552
CRCs and SOURCE_COMMIT; ZIP SHA256
`425b0e83673d9296368bc90c5d125b2bc97f9d4d50ab0c4dcbf395e05f623d35`.
AArch64 encoderd SHA256 is
cc192fb9e439b418ccd0b8804086a5854e0f71726a185c0a9316046b5c249094; codec
and V4L trace hashes remain c777ca70abeebba3b7ac138d70fb9f4a510bb1ed7d9dd13537f8d689ce465672 /
6236df4b53b99fbf9d707eb46f3d1a359176d9ff8715086d5deb57e811b73cb2.
Generic artifact12335202195 has83 valid CRCs, ZIP SHA256
`8c6ebec18f0302e0bf9117bec8e6a49bd4a12f47c723f23f1676feb14e424d36`;
dashcam GNU/musl AArch64 hashes are
42c142ff082f0db2cb109cda986621adc8f68be21d015e9f55b8edef112b5d77 /
2bcbf402f881539c5b10de663b844a52b6b5af629c40e40062233292a7cc6c04.
These remain generic build artifacts rather than sealed AGNOS/device evidence.
