# Verified model catalog generation

`original.json` retains two complete, non-executable metadata catalogs and the
output of the exact previous workflow's Python `current.json` generation read.
The only architecture-dependent graph field is captured for x86_64 and aarch64;
all file bytes, index digests and original stdout/stderr are independent fixture
data captured before the native CLI implementation.

The workflow already validates the catalog with `model-run --check-catalog` before
using its generation. The new `--catalog-generation` repeats the same validation
and prints its verified index identity without loading/executing model kernels.
It preserves output for accepted catalogs, while rejecting a catalog tampered
between validation and metadata use. It is intentionally not a general Python
JSON-value printer: malformed/unverified catalogs remain invalid inputs.

The test first validates the unchanged fixture, compares native stdout with the
captured original without Python on PATH, then corrupts the index and verifies
both native catalog commands reject it without success output.
