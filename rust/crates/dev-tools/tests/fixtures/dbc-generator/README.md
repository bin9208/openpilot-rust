These expectations were captured by executing the unchanged Python generator and
stager before any native implementation. They are test oracles, never runtime
inputs to the native generator. Original licenses and provenance remain with the
procedural policies and DBC sources.

`generate-all-original.json` records all 45 original output names, exact SHA-256
and byte lengths (992,535 bytes), and 73 input identities. Full raw original
outputs remain in ignored scratch for the later byte-for-byte native comparison.
The manifest contains thirteen generator Python sources plus the staging script;
only those fourteen sources may be absent after verified retirement. Retained
DBC inputs and the Ford shell generator remain mandatory in the baseline test.

`compose-original.json` has fifteen generic cases covering include order and
repetition, nonrecursive includes, exact regex matching, universal newlines,
Unicode/BOM, filename replacement, parent-relative includes, and failure ordering
with partial output. `create-all-original.json` has four filesystem cases for
walk/filter behavior, collision ordering, cleanup-before-generation and hidden
output preservation. `stage-original.json` has five cases captured from the
actual original stage main with an explicit generator-result seam: existing
files/directories/valid links are preserved; exclusive creation rejects dangling
links; prior successful writes survive later failure; a generation error occurs
before any target write. Full generation is independently covered by the 45-file
baseline rather than the stage seam.

`helpers-original.json` contains fifteen actual Tesla and Hyundai helper calls,
including non-default and negative addresses and Unicode/quote-bearing names.
`chrysler-original.json` contains four executions of the unchanged remapper with
modified common input, missing mapping addresses and malformed address tokens.
Valid large outputs are bound by exact hashes and sizes; small partial failure
outputs retain exact bytes. `provenance.json` binds the capture drivers, sources
and fixture bytes. Capture scripts and raw artifacts are under the ignored
`.analysis/scratch/2026-10-18-dbc-generator/` directory.

The native implementation is now present but has not been compiled. Native tests, actual full generation,
source nonmutation, CLI behavior, empty-PATH execution, caller migrations and
post-retirement proof remain pending. Arbitrary foreign Python plug-ins are not
implicitly converted; native procedural programs require explicit registration.

`paths-original.json` adds two real surrogateescaped filesystem-path controls:
raw source/output directory bytes succeed, while a raw filename fails during
UTF-8 output encoding after retaining the original partial header bytes.
`cli-original.json` records three actual original generator-main subprocesses on
private complete source copies: successful full generation, ignored arbitrary
argv, and a missing include failure with partial output. Their raw stderr/stdout
remain in scratch; native CLI subcommands are intentional explicit plumbing, not
a claim that ignored original argv selected operations. No Python subprocess is
permitted as a native generator fallback.

`glob-original.json` adds four unchanged-original directory-pattern controls.
Python glob interprets metacharacters inside the supplied directory arguments:
an unmatched `out[1]` retains its literal stale file; a matching sibling `out1`
loses its stale file instead; `out?` can match its own literal question-mark
name; and unmatched source brackets suppress the procedural phase before a
missing generated include fails composition. These four expectations remain unchanged as evidence of inherited bug #375.
The native implementation deliberately corrects this behavior: directory
arguments remain literal, cleanup is confined to the requested output, and
metacharacters cannot select a sibling or suppress the requested procedural
source. Focused bracket/question/star regressions assert that correction.
`literal-correction-original.json` binds exact outputs of the unchanged original
Chrysler script and direct create_dbc operations on the bracket source; it is
explicitly a corrected-orchestration control, not unchanged create_all parity.

Native CLI coverage additionally requires complete in-place `--all-programs`
generation after removing all Python and Rivian's formerly Python-only directory
from a copied source tree. All 45 output hashes remain the independent original
expectations; both Chrysler intermediates and the recreated native Rivian output
must exist. Explicit subset selection is separate; combining `--all-programs`
with `--program` is rejected before mutation. `program-outputs` is derived from
the live native registry for SCons, rather than from retired filenames or fixtures.

`errors-original.json` records four unchanged-original exception objects for the
opt-in native machine-error boundary: missing include under a raw-byte source
path, invalid main UTF-8, truncated include UTF-8, and a filename containing
Unicode plus a surrogateescaped byte. It retains original exception kinds,
errno, decode/encode object bytes, Unicode start/end/reason, and partial outputs.
The encoding object is the entire failed header, not just the invalid filename.
These controls let the still-pending Python facade reproduce built-in exception
types without parsing human diagnostics or reimplementing DBC generation.

`dbc-generator --json-errors COMMAND ...` emits one JSON object on stderr only
on failure: `kind`, `errno` (nullable), `message`, optional `path_bytes`,
`decode_bytes`, `encode_bytes` (arrays of byte integers). `path_bytes` preserves
OS path bytes for IO errors; decoding/encoding payloads preserve the raw object
of the failed operation. Ordinary stderr, exit codes and successful stdout stay
unchanged. Only a leading exact flag enables this protocol; repeating the flag
or omitting the command is an argument error in the selected JSON mode.

`chrysler-digit-limit-original.json` binds six unchanged-original remapper controls
for CPython's decimal integer conversion limit: 4,300 digits succeeds; 4,301
digits fails with `ValueError`, including leading zeroes and Unicode digits.
Signs and underscores do not count toward the limit. Complete output hashes
preserve the 4,300-digit success bytes and the exact partial DT output plus absent
HD output on failure. The permanent native regression reproduced the missing
limit before the repair (`ascii-4301`: success versus original `ValueError`).
The narrow repair counts normalized decimal digits before removing leading
zeroes; the combined repaired gate remains pending. No prior capture changed.
