# Independent drawing captures

These compressed JSON documents contain calls observed while running the
unchanged original helpers. They are test oracles only. The production reference
does not read them.

- `family-original-draw.json.gz`: all 142 PNG-producing cases from the 26
  immutable family-text fixture files (220 cases including errors).
- `reference-original-draw.json.gz`: all remaining 243 PNG-producing cases:
  Forms 98, Render 92, Network 53, across 34 original fixture files.

Each document binds the original helper digest, original provenance document,
whole original fixture bytes, fixture path plus case name, exact scene bytes,
original PNG digest, process exit, and state artifact. Original state bytes and
PNG digests must match before a capture is admitted. Cases with a PNG followed by
JSON encoding failure remain included.

The calls retain original text after the actual C string conversion, coordinates,
colors, spacing, clipping, font atlas and source files, atlas page hashes, glyph
codepoints, mipmap policy, asset texture sizes and bytes, emoji RGBA, matrix
operations, shaders, target readbacks and encoder frame readbacks. Every original
font measurement is checked at the binary32 ABI boundary during replay.

At test time the mechanical helper replays these immutable calls with the same
raylib and renderer environment as the independent native widget implementation.
PNG bytes, decoded i64 samples and recorded RGBA frames are compared exactly.
Original PNG and recording digests remain immutable capture provenance; they are
not an allowlist of renderer-dependent outputs. Existing state, error, file,
process, recording argv and byte-count checks remain separate.

The support code pins the uncompressed capture digest and whole fixture hashes.
Mutation tests reject changed capture bytes, inputs, missing cases and changed
fixture bytes. Input binding is iterative and exact, including Python surrogates,
arbitrary integers, booleans and float bits; source state comparison tolerances
are not used to admit a changed input.

The original helpers are recovered into ignored scratch from their immutable Git
blobs for capture only. They are never a production fallback. Raw source captures
and logs are kept outside Git.
