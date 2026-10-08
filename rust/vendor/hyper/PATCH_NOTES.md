# Carrot Web header value compatibility

Base: crates.io `hyper` 1.11.1, copied from the local Cargo registry. The package's `.cargo_vcs_info.json` records upstream revision `6371cd425017155f7fbecef0e57b218edbe6a93a`. Upstream license and package sources are retained.

The HTTP/1 server Builder option `preserve_raw_conditional_headers(bool)` defaults off. It adds a `hyper::ext::RawConditionalHeaders` Request extension containing the original Range, If-Range, If-Match, If-None-Match, If-Modified-Since and If-Unmodified-Since values, including trailing ASCII space/tab. The original immutable, already-framed request head supplies these values. The ordinary HeaderMap and client path are unchanged.

Carrot Web needs this because aiohttp 3.13.3 preserves trailing whitespace in those headers. For example, a trailing space in `Range: bytes=0-1 ` causes its strict byte-range parser to return 416; httparse's normalized HeaderMap otherwise makes that header appear valid.

Project-owned real HTTP regressions and source comparisons live in `rust/tools/carrot_server_static.py` and the `static_web` example. Upstream ParseContext test literals supply the new server-only flag as false; this patch does not change their original scenarios. No new external dependency is added.

## Content decoder transport compatibility

`close_after_response(bool)` defaults off. When enabled for a server connection, `hyper::ext::CloseAfterResponse` on a successful response closes the connection after the complete response. The marker is consumed only after headers are encoded, then the existing keepalive state and `Encoder::set_last(true)` handle completion. It does not insert a Connection header or change framing. Unmarked responses, defaults and client connections retain existing behavior.

`read_buf_exact_size(usize)` exposes the existing exact read-buffer strategy on the HTTP/1 server builder; omission retains the adaptive default. Carrot Web selects256KiB, matching Python3.12's asyncio selector socket maximum receive size. This is a maximum read size rather than guaranteed coalescing or a timer. The existing private conn/io method cfg now permits server use too.

Server HTTP/1 `Incoming::poll_buffered_frame` reads its existing framed data/trailer channel or EOF without signaling WANT_READY. It performs no socket read and returns Pending for HTTP/2/FFI bodies; ordinary Body::poll_frame and all default behavior are untouched. This lets the application preserve one decoder state while inspecting already available input. Parser ordering, partial input, chunked EOF and Expect handling require actual source comparisons before this transport wave is complete.

The approved patch and its source/native receipts are retained under `.omo/evidence/carrot-server-225-resume/request-decoding/`; no additional unsafe boundary or dependency is introduced.
