ureq-proto 0.6.4 is copied from its unchanged Cargo registry package.
Original package checksum: `f86fd172ccca569e458f61b6bdd6220965a9ef36e672a6852953b51a0e1583be`.
Original source commit: `bd8ec535c6567ab59683446b18fd6c36748afb16`.
The Apache-2.0/MIT license files and original source checksums are retained in
`LICENSE-APACHE.txt`, `LICENSE-MIT.txt` and `UPSTREAM.json`.

The only source additions expose the existing `Dechunker::CrLf | Size` state
through `BodyReader` and `Call<RecvBody>::is_at_chunk_data_end`. This distinguishes
completed chunk data before trailing CRLF from a partially received chunk.
The parser, transitions, framing policy and default read behavior are unchanged.
The separately vendored ureq enables boundary reads only when a caller explicitly
requests raw-chunk progress, for Carrot server heartbeat's urllib error messages.
