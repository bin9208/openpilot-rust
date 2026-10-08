# Carrot Web header value compatibility

Base: crates.io `hyper` 1.11.1, copied from the local Cargo registry. The package's `.cargo_vcs_info.json` records upstream revision `6371cd425017155f7fbecef0e57b218edbe6a93a`. Upstream license and package sources are retained.

The HTTP/1 server Builder option `preserve_raw_conditional_headers(bool)` defaults off. It adds a `hyper::ext::RawConditionalHeaders` Request extension containing the original Range, If-Range, If-Match, If-None-Match, If-Modified-Since, If-Unmodified-Since and Expect values, including trailing ASCII space/tab. The original immutable, already-framed request head supplies these values. The ordinary HeaderMap and client path are unchanged. Expect is retained because aiohttp rejects100-continue with trailing whitespace and ignores an empty header; the first duplicate value remains selected.

Carrot Web needs this because aiohttp 3.13.3 preserves trailing whitespace in those headers. For example, a trailing space in `Range: bytes=0-1 ` causes its strict byte-range parser to return 416; httparse's normalized HeaderMap otherwise makes that header appear valid.

Project-owned real HTTP regressions and source comparisons live in `rust/tools/carrot_server_static.py` and the `static_web` example. Upstream ParseContext test literals supply the new server-only flag as false; this patch does not change their original scenarios. No new external dependency is added.

## Content decoder transport compatibility

`close_after_response(bool)` defaults off. When enabled for a server connection, `hyper::ext::CloseAfterResponse` on a successful response closes the connection after the complete response. The marker is observed before header encoding consumes extensions; only successful encoding changes the existing keepalive state and `Encoder::set_last(true)`. For a marked HTTP/1.0 parser response, automatic Connection:close insertion is skipped; caller-supplied headers are retained. Unmarked responses, defaults and client connections retain existing behavior.

`read_buf_exact_size(usize)` exposes the existing exact read-buffer strategy on the HTTP/1 server builder; omission retains the adaptive default. Carrot Web selects256KiB, matching Python3.12's asyncio selector socket maximum receive size. This is a maximum read size rather than guaranteed coalescing or a timer. The existing private conn/io method cfg now permits server use too.

Server HTTP/1 `Incoming::poll_buffered_frame` reads its existing framed data/trailer channel or EOF without signaling WANT_READY. It performs no socket read and returns Pending for HTTP/2/FFI bodies; ordinary Body::poll_frame and all default behavior are untouched. This lets the application preserve one decoder state while inspecting already available input. Parser ordering, partial input, chunked EOF and Expect handling require actual source comparisons before this transport wave is complete.

`prefetch_buffered_body(bool)` defaults off and is server-only. Before dispatch it runs the same body Decoder against only the existing BytesMut read buffer, where empty memory means Pending. Frames are queued on the same Incoming and its length is decremented once upon consumption. Actual EOF drops the existing sender; partial input retains it and the decoder state. Continue state is preserved without informational output or body-demand signaling. No socket poll, timer, channel-capacity change, content decoding or new framer is added; pipelined bytes remain after the existing decoder reaches EOF.

`application_continue(bool)` defaults off and installs a one-shot `ContinueSignal` request extension only on opted-in server connections. Its existing common watch sender remains retained by the dispatcher even if an immediately-ready service drops the request. The dispatcher checks the signal before and after service polling, including Pending, then queues the existing100 bytes and resumes the same Continue decoder as Body. Automatic demand output is disabled only for that opted-in server path. The application decides after content-parser prefetch; defaults, clients and H2 are unchanged.

For only the existing buffered Decoder Start/InvalidInput/missing-size-digit failure, `BufferedChunkStartError` retains the last consumed byte and remaining buffered line through CRLF. Copying occurs only after rejection within that existing feed. The wrapper preserves native Display and the original io::Error source; the application recognizes its typed cause to preserve aiohttp's parser response diagnostic. Other framing/IO errors and first payload-error precedence are unchanged. No new hex parser or acceptance policy is introduced.

The approved patch and its source/native receipts are retained under `.omo/evidence/carrot-server-225-resume/request-decoding/`; no additional unsafe boundary or dependency is introduced.
