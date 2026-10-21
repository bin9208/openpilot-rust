# Reuse owned V4L packet scratch (#447 / #296)

The native dequeue worker allocated a new Vec for each normal encoded packet, then copied that payload into Cap’n Proto. It now retains one owned scratch buffer and copies each packet into its exact prefix. The initialized length and capacity retain their high-water marks, so a shorter packet followed by a longer packet does not repeat zero initialization within the already initialized range. Memory can remain at the largest packet seen until this worker exits; Vec capacity can exceed that length.

Mapping still returns owned copies or copies into owned destination storage. No Rust reference to externally written DMA storage is exposed. The original DMA-to-owned-memory copy and later Cap’n Proto copy remain. Codec headers, EOS, packet bounds, timestamp/metadata checks, publication and requeue order are unchanged.

The new native test compares variable, empty and maximum payloads against the existing Mapping::copy method, compares fingerprints, rejects an oversized destination without changing its sentinel, and measures Rust allocator calls after warmup. It records both paths’ copied-byte counts and retained capacity. These are copy-boundary measurements, not full encoding, recording or vehicle timing results. The main hosted encoder job retains its package log, complete codec/V4L/runtime comparisons and strict Clippy. The affected ARM build remains pending. Local checks are syntax, formatting and diff review only.


## Accepted hosted copy and encoder proof (2026-10-21)

[Run39539787018](https://github.com/bin9208/openpilot-rust/actions/runs/39539787018) succeeded with exact checked source `5bcfae4482d9c4dcb1da46e1086bea609e0c9688`; diagnostic workflow HEAD was `72ecab9af103e1b5a8a8281f334bb72856a6380a`. Artifact12318804134 ZIP SHA256 is `9d11ba8fae4a050ecb616e174ea5e35229b2ea9e380a304e52f44c59fddc6642`.

Parent and independent reviewer inspected the retained package log. The warmed copy test measured70 allocator calls in the old path and0 in the scratch path, with equal93,220 copied bytes and4,096 bytes maximum logical payload/retained capacity in this fixture. Five codec cases,32 scripted V4L cases and10 runtime cases passed full packet and metadata comparisons; native package/IPC tests and strict Clippy passed. These results measure Rust allocations at the owned-copy boundary only. Both data copies remain; no CPU, hardware VIDC latency or vehicle recording improvement is claimed. Current full PR ARM validation and user device acceptance remain separate.
