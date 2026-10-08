# Carrot Web header value compatibility

Base: crates.io `hyper` 1.11.1, copied from the local Cargo registry. The package's `.cargo_vcs_info.json` records upstream revision `6371cd425017155f7fbecef0e57b218edbe6a93a`. Upstream license and package sources are retained.

The HTTP/1 server Builder has one default-off option, `preserve_raw_conditional_headers(bool)`. It adds a `hyper::ext::RawConditionalHeaders` Request extension containing the original Range, If-Range, If-Match, If-None-Match, If-Modified-Since and If-Unmodified-Since values, including trailing ASCII space/tab. The original immutable, already-framed request head supplies these values. The ordinary HeaderMap, body decoder, content length, transfer encoding, keepalive and client path are unchanged.

Carrot Web needs this because aiohttp 3.13.3 preserves trailing whitespace in those headers. For example, a trailing space in `Range: bytes=0-1 ` causes its strict byte-range parser to return 416; httparse's normalized HeaderMap otherwise makes that header appear valid.

Project-owned real HTTP regressions and source comparisons live in `rust/tools/carrot_server_static.py` and the `static_web` example. Upstream ParseContext test literals supply the new server-only flag as false; this patch does not change their original scenarios. No new external dependency is added.
