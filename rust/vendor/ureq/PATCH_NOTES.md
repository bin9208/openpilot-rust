# Local ureq 3.4.2 patch

This directory copies the existing cached crates.io package with the code addition below and removal of one trailing blank line in CONTRIBUTING.md for the repository whitespace check. Original Cargo metadata, licenses and `.cargo_vcs_info.json` are retained; upstream source identity is `2e9ef24a80e1e7ecd0e6604f98ba49aceb7ec322`. The repository's issue #225 Mapbox token adapter needs urllib's raw capped-response behavior while reusing the existing per-socket HTTP transport.

`src/body/mod.rs` adds one method, `Body::as_raw_reader(&mut self) -> impl std::io::Read + '_`. It returns the existing borrowed `BodySourceRef`, retaining HTTP transfer decoding/framing and body-handler completion/drop behavior, while bypassing content decompression and charset conversion. A caller can cap actual bytes with `Read::take`. Existing `as_reader`, consumers, parser, transport and default decoding remain unchanged. No unsafe code or new dependencies are added by this patch.

The focused validation records default-decoded versus raw gzip/Brotli responses, partial-read/drop followed by the next request, and the Mapbox adapter's source comparisons under `.omo/evidence/225-mapbox-tokens/`. Evidence remains local. The workspace patch entry and lock/dependency wiring are owned by the coordinating server worker.
