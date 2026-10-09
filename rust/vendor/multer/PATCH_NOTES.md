# Local source compatibility patch

Exact base: crates.io `multer` 3.1.0, checksum recorded in Cargo.lock and
verified against the cached `.crate` archive, registry source copied without
dependency installs.
Upstream: https://github.com/rwf2/multer; MIT license retained in LICENSE.

This local delta preserves the aiohttp 3.13.3 multipart headers used by original
`features/params.py::api_params_restore`:

- Append duplicate part headers so HeaderMap.get selects their first value.
- Size httparse header storage from the already framed header block rather than
  reject more than 32 headers. Original Python HeadersParser stores a max_headers
  default but does not check the count in this path.
- Check original header name and value byte lengths against 8190 before converting
  into HeaderMap. Preserve the spelling and count trailing SP/HTAB from the same
  header block, matching Python HeadersParser before its final whitespace trim.
  A typed HeaderTooLong error leaves HTTP status/error conversion to the caller.
- Declare the existing nightly configuration to rustc check-cfg. This does not
  enable nightly behavior or suppress lint checking.

No body framing, field reading, decoding, dependencies, default body constraints,
client parser, or other upstream behavior is changed. Regression evidence is in
`.omo/evidence/carrot-server-225-resume/multipart/native-unmodified-normalized/`.

Trailing whitespace in the upstream README.md is normalized for the repository
whitespace check; the documentation content and license are unchanged.
