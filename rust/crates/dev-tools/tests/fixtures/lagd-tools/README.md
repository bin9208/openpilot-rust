Independent original lagd QA captures taken before native implementation.

`numeric-original.json.gz` contains all95 fixed source numeric inputs/outputs:
padding, peak interpolation, smoothing, correlation, delay/candidate metadata,
block history and calibrated pose. `numeric-inputs.jsonl.gz` preserves the exact
original NumPy-seeded request stream. The original fixed tolerance file remains
live and unchanged; discrete decisions are never covered by a float tolerance.

The three `loop-*-original.json.gz` files contain all3703 original frame inputs
AND every source state snapshot, including nonpublication masks/recovery state.
They preserve normal1201/recovery1301/ordering1201 frames, publications241/261/241
and persistence at frames0/1200. No native candidate supplied an expectation.

`cache-original.json.gz` preserves all14 source restore/remove cases, including
unknown enum/nonfinite/corrupt/negative-count values. `packet-original.json.gz`
preserves all13 source status/clipping/standard-deviation/progress cases, with
the original caught schema exception recorded separately from the error marker.

`manifest.json` fixes all compressed bytes and accompanying source/capture metadata.
`source-capture-receipt.json` binds decompressed bytes, CPython3.12.15,
NumPy2.5.3/dd88c0c19b54ad9ed3533224221285bf0873249a and pycapnp2.1.0.
`source-pins.json` fixes the nine QA originals, retained runtime helpers,
transformations, schemas, NumPy/PocketFFT provenance and preimplementation budget.
The scratch capture invokes unchanged source functions; the loop input-building
prefix is extracted from the unchanged original checker before its native spawn.

The independent original baseline subsequently ran all six unchanged checkers
against six freshly built native candidate executables and retained 55 artifacts.
The source-derived reference foundation now passes 14 Rust tests and strict Clippy:
all 95 numeric cases, all 3,703 exact frame batches and full loop snapshots, all
14 cache and 13 packet cases, and the admission controls described below. The
source and capture scripts are in ignored `.analysis/scratch/2026-10-18-lagd-tools/`.
These are host reference gates. The six native checker/controller entrypoints now
also pass the independent process comparisons described below. The nine QA tools
were removed only after the complete pre-deletion proof passed. Runtime lagd
policy and its retained Python source files are unchanged.

Separate source-only boundary captures are pinned by `boundary-manifest.json`;
the initial core manifest/expectations remain unchanged. `generic-original.json.gz`
records121 original calls (59 successes/62 errors), including raw nonfinite JSON,
Unicode/underscore/NUL numeric strings, mask dtype/indexing, shapes and failures.
`dtype-parent-original.json` preserves five separately captured weighted-mask,
integer-storage and string-rejection controls unchanged. All 121 admission cases
and five independent controls pass. The additional immutable scalar-kind and dtype
inference files each preserve six original controls for boolean padding identity,
NumPy wrapping integer arithmetic, unsigned/float/object promotion and indexing.
The 25 live provenance inputs are each tested for missing and changed content.

`cli-original.json.gz` contains42 actual original CLI invocations across all six
checkers. `checker{,-lines,-phases}-original.json.gz` contains71 original checker
contracts with only subprocess.run replaced by original-source response bytes or
a process/decode failure. Corresponding `*-bytes.json.gz` files deduplicate exact
request/response/output-file bytes. All original assertions and full inputs remain.
The first stream CRLF/NEL probes deliberately retain two final terminators and
reject empty JSON lines; separate single-terminator controls preserve successful
Python splitlines behavior. Later-failure controls preserve partial loop stdout
and files, numeric report-before-failure, and packet error-membership behavior.
These are test-only transport fixtures, never a production expected-policy table.
These fixtures preserve original admission, ordering and failure expectations;
production comparators do not look up fixture answers.

`reference-manifest.json` binds 61 original constructor controls and 16 additional
loop wire/configuration controls captured before implementing these references.
`loop-admission-manifest.json` binds 25 independently captured bytes-conversion and
empty-iterable boundaries. All these controls pass in the 14-test foundation.
No original expectation or fixed numerical tolerance was regenerated from native
results. The reference uses source-derived algorithms with generated cereal wire
mechanics and the pinned external PocketFFT mathematical kernel; the candidate
runtime estimator supplies no expected value. Exactly nine retired QA source
paths may be absent; all sixteen retained source inputs remain mandatory. Every
present original is still checked against its immutable source hash, including
reintroduced originals. Changed files, directories and dangling symlinks fail.

The full native suite passes 34 tests across fifteen targets, including 42 original
CLI cases, 71 original batch/process contracts, 24 live failure boundaries, three
healthy process controls, three raw CR/LF controls and a real blocked-read SIGINT
control. The separate CI suites pass 53 tests; strict Clippy passes for the library,
six binaries and all eighteen selected test targets. The same full 87-test selection and strict Clippy also pass after deletion,
including all fifteen family targets and all three CI policy targets.

Before deletion, all six unchanged original checkers ran against six freshly built
current-root candidate ELFs. The same six native tools and six candidate ELFs were
frozen, then all six native commands passed both before and after removal of the
nine originals. Each phase covers 3,831 aggregate cases: numeric 95, loop 3,703,
cache 14, packet 13, daemon 4 and Params I/O 2. The daemon cases exercise 1,201
learning frames, corrupt persistence, publication cadence and CarParams-wait
SIGTERM; Params I/O uses two real owned directories. No original was restored or
executed after deletion.

Both native proof phases ran with PATH=/nonexistent and execution tracing. Each
recorded 34 executable launches and zero Python attempts, with the twelve sealed
ELFs and archived original Git blobs checked. All 45 artifact paths, command
stdout/stderr, native values, non-pose fields, classifications, key order and
formatting are preserved. Thirty-one artifacts are byte-exact. The remaining
artifacts differ only in named host-clock fields or the 121 independently derived
source-pose floating values allowed by the unchanged preimplementation 1e-12
absolute/relative budget. This is not a bit-exact pose claim: the independent
reference and captured NumPy/BLAS path differ in floating reduction behavior. No
production tolerance or original captured byte was changed. Raw event comparisons
exclude only the actual Event.logMonoTime data word and bind it to the corresponding
publication JSON. Cadence bounds, cardinality and every other event byte remain
strict.

Native process ownership tests retain group identity until descendant cleanup,
then reap the leader. CLI SIGINT is re-raised only after owned resources are
released; raw binary Params output preserves CR/LF while peer text retains its
original newline translation. Native RAII additionally cleans its newly owned
namespace when spawn fails before the original try/finally region. This cleanup
improvement is explicit and does not alter successful estimator policy.
