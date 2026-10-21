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

The maintained support-runtime gate keeps all original upload/Params/catalog/
metadata/lifecycle and Git ownership comparisons, and adds package tests and
strict all-target Clippy. Hosted functional proof, allocation values and affected
ARM verification are pending. No CPU, bandwidth, latency or device result is
claimed from source inspection.
