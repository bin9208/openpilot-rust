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
