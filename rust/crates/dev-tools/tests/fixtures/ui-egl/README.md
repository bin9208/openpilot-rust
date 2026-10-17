# Independent dynamic EGL reference evidence

The native checker uses a separate Rust reference process for every one of the
seven original cases. Expected runtime states and traces come from that independent
policy operating against the actual supplied libraries. They are not golden
outputs substituted for dynamic library behavior and are not derived from the
candidate's EGL lifecycle implementation.

The optional `openpilot-dev-tools/egl-reference` feature gates both native commands
and these integration tests. Default dev-tools builds retain their existing
independence from raylib. The feature uses the existing audited startup-ui CXX
boundary and the pinned native SDK/raylib build inputs. Runtime proof here is
Linux x86_64 only; it does not establish ARM or real display/device equivalence.
The private ABI fixture never imports a camera image or initializes a real display.

## Source policy and raw ABI separation

`ui_egl_reference.rs` owns initialization order, current-display handling,
proc-address/library fallback, one NOT_INITIALIZED retry, NV12 attributes,
three image operations and descriptor ownership. The separate C++ ReferenceApi
only loads libraries, resolves typed symbols and performs individual ABI calls.
Opaque display/image tokens are validated before calls. It never invokes the
candidate Context/EglApi lifecycle policy. The public Rust close wrapper consumes
OwnedFd; no arbitrary raw-fd close is exposed publicly. Original destroy ignores
close errors, while a rejected create propagates them. Partially loaded libraries
remain owned through the observable final descriptor count and state print.

The checker intentionally adds a fail-closed drift guard after output mkdir and
before compilation: retained `openpilot/system/ui/lib/egl.py` plus complete cffi
and pycparser lock records must match `policy-pins.json`. Changes require independent
reference revalidation. Removed helper/checker hashes remain immutable provenance
in `original-reference.json`; runtime checking never requires those removed files.
Generic `egl.py` and its arbitrary runtime functions remain pending conversion.
Python interpreter/import failures are removed dependencies, not simulated native
success/failure evidence.

## Independent captures and checks

`original-reference.json`: unchanged original helper, CPython 3.12.15/cffi 2.1.1,
interpreter/backend/source/compiler hashes, and seven actual source states/traces
against a freshly compiled private ABI fixture. The source helper's only maintained
caller was the old checker. Existing documentation describes the checker's borrowed
and owned modes, both preserved through inherited EGL_OWNED_IMAGE.

`original-variants.json`: 35 actual source executions against five separately
compiled ABI variants: alternate GLES calls, missing core symbol, missing destroy
extension, failed retry initialization, and partial load retaining a constructor's
FD. The latter returns fd_delta=1 before unload, detecting premature ownership loss.

`original-closed-fd.json`: 14 actual source executions and prior native failure
codes for foreign destroy/create closing the duplicated FD. Original successes
previously aborted the first native probe with SIGABRT; the ownership-transfer
repair now matches success and expected runtime close-error exits/traces.

`original-checker.json` and content-addressed gzip blobs: 41 whole unchanged old
checker runs using controlled compiler/child peers. They pin merged build logs,
partial artifact ownership, relative symlink behavior, strict UTF-8/universal
newlines, state/trace comparison, delayed assertion, JSON depth/receipt phases and
partial printed rows. The actual compiler-produced ELF bytes stay outside Git;
fast peer tests check its ownership while separate actual builds compare its bytes.
`original-cli.json` contains nine independent argparse admission/error captures.

Six focused tests also reject unknown/stale display/image tokens, malformed or
nonterminated attribute slices, and source/dependency drift before compiler spawn.
`actual-proof.json` records 12 whole original/native comparisons with freshly built
current Rust egl_trace and fresh real g++ fixture builds. Every artifact (including
ELF bytes), stdout and stderr matched, across borrowed/owned modes, raw Unix
candidate/output/root-alias paths and child-verified raw inherited environment.
Noncanonical existing invalid and alternate valid GLES libraries both passed.
`no-python-proof.json` records two whole native commands with PATH containing only
a compiler shim; its compiler subprocess privately receives the native SDK PATH.
