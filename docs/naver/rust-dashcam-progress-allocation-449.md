# Dashcam progress filename lookup (#449)

Every progress callback previously allocated a fresh filename String for the
flat `(segment, filename)` map, including retry chunks for an existing file.
The candidate uses per-segment maps with borrowed filename lookup. First sight
still allocates a filename; nested maps may add cold-start allocations. Segment
separation, retry high-water logical bytes, transmitted bytes, clock call order,
sample eviction, progress patches, cancellation and final results are preserved.

`upload_one` uses the factored production bookkeeping. A retained prior closure
compares complete JSON patches and state across repeated chunks, retries, empty
names, duplicate names across segments, oversize sent values and zero totals.
A separate measurement compares cold and1,000 warmed events with the same JSON
observer. The test starts its own bounded single-test process so ordinary
parallel Cargo tests cannot contaminate the global counters. It requires one
saved filename allocation per warmed event and reports cold results separately.
The counter dependency is pinned dev-only stats_alloc0.1.10, with no dependencies;
its registry checksum comes from the official sparse index. Local checks read
only metadata and checked syntax/format/diff; no dependency installation or build
ran locally. Runtime unsafe-code lint policy remains unchanged.

The [hosted support-runtime job](https://github.com/bin9208/openpilot-rust/actions/runs/39550724394/job/119059059811)
passed on checked-out source `4c876dc7da80629b81a166bc4b0810f1621d2d42`
(diagnostic workflow head `5c50cf08406378f87731a5b4997acf122c5772c4`).
Its retained artifact is `12324177889`, ZIP SHA-256
`0c962cfb5fb0518ae3383f6ea8a75b876283fa92e779d972fbf3e770bc85abb7`.
The ZIP passed CRC inspection. `support-runtime/dashcam-progress-tests.log`
records both unit tests passing, including the isolated allocation child;
`dashcam-progress-clippy.log` records strict Clippy completion. The whole job
also passed the maintained original/native HTTP, upload, Params, catalog,
metadata, Git capture, connection ownership and lifecycle steps.

For one cold event, the previous bookkeeping recorded 4 allocation calls and
475 requested bytes; the nested-map path recorded 5 calls and 687 bytes. For
1,000 warmed events with the same JSON progress-patch serialization, the previous path
recorded 3,000 calls and 303,128 bytes; the current production helper recorded
2,000 calls and 273,128 bytes. The 1,000-call and 30,000-byte differences equal
one copied 30-byte filename per warmed callback. Both paths emitted the same
semantic fingerprint, `3894085801202778587`, and the test compared complete
progress patches and state. These are Rust allocator requests in a hosted test,
not a measurement of HTTP transport, peak memory, CPU time, bandwidth or device
performance. Affected ARM/device verification remains pending.

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
ARM performance evidence. New head917cd7db has its own full PR run39567841179;
its required checks and updated ARM build remain pending.
