# Panda allocator-padding controller

This is a source-only development-tool conversion of exactly
`rust/tools/check_pandad_safety_padding.py`. The original file remains unchanged
and executable at its original path. Its shared `invoke()` / `operation()`
dependency, `check_pandad_safety.py`, remains available to its other consumers.
The only current executable caller is `check_pandad_ci.py`'s safety branch.
No Panda runtime policy, device transport, workflow or existing caller is changed.

`contract.json` records inspection of the unchanged source. It is **not an
executed capture** and contains no candidate responses. The native controller
asks the supplied original C++ probe to encode CarParams dynamically, retains
the complete wire and all four requests, and independently compares both probes.
Two full-message controls must agree after removing the original-only allocator
counter. The truncated controls must expose the original allocation fill in the
last safety parameter while the Rust policy rejects both with the same two
no-output commands. This preserves the original deliberate malformed-input
difference; it does not call malformed behavior equivalent or assert device safety.

The native controller retains three original invocation phases, each with its
original 90-second timeout: source CarParams encoding, four source safety rows,
and four native safety rows. Original input/stdout/stderr filenames and exclusive
output-directory behavior remain. Invalid UTF-8 streams fail rather than being
silently replaced. JSON request filesystem paths use the existing lossless
Python-filesystem serializer. Missing executables still fail at the corresponding
spawn phase after its input artifact is saved. Native CLI accepts the same three
required options, help, and argparse option grammar; native help/error framing
is deliberately Rust-owned (help 0, argument error 2, execution failure 1).

Additional ownership protections use the existing supervised runner with signal
cancellation, input source/probe descriptor seals and atomic no-clobber receipts.
Final stdout is checked before PASS publication, seals are rechecked afterwards,
and a held receipt descriptor prevents inode recycling before late-cancel cleanup.
Those checks detect observed replacement/mutation, not an atomic snapshot against
arbitrary concurrent modification. The seven admission hashes cover the controller,
shared helper, source fixture and direct PandaSafety/Params source files. The
source-builder manifest separately records its fuller C++/schema input map; this
controller does not mislabel its seven pins as a complete compiled closure.

`pandad_safety_padding_check` contains ten source-derived contract/control tests.
Its replay responses are explicitly test doubles for controller assembly, not an
oracle or claimed original observations. The separate mandatory
`pandad_safety_padding_reference` target has three actual-process CI tests:

- Invoke the retained Python controller and native checker against the same
  fresh original C++ probe and native example, with native PATH disabled. Compare
  whole reports and stdout reports, all probe stdout/stderr bytes, and complete
  parsed input rosters. Only their separately owned Params-root paths differ.
  Revalidate probe hashes and the existing source-builder's declared source map.
- Compare actual help/argument exit statuses and both missing-executable artifact
  phases against the unchanged original controller.
- Cancel after an immutable negative-control launcher acknowledges its live PID;
  retain pidfds, require prompt owned-probe exit and absence of a PASS report.

Mandatory CI variables are `PANDAD_PADDING_SOURCE_BINARY`,
`PANDAD_PADDING_NATIVE_BINARY`, `PANDAD_PADDING_SOURCE_MANIFEST`,
`PANDAD_PADDING_PYTHON` and an existing `PANDAD_PADDING_TEST_OUTPUT` directory.
The blocked-probe launcher only execs `/bin/sleep`; it never returns a fabricated
successful safety response. Raw pair artifacts remain in the CI evidence root.

Status: formatting/syntax review only. No local build, test, Clippy, original
capture or runtime probe was run. All 13 tests and live parity are CI-pending.
Neither original Python file is eligible for deletion from this source-only work.
