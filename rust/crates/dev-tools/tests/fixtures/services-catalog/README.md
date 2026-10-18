# Independent service catalog checker contracts

Captured on 2026-10-18 from the unchanged `rust/tools/check_services.py` and the
original `openpilot.cereal.services` / full pycapnp Event schema, before creating
any native replacement. The source SHA-256 pins, existing installed dependency
identity, capture script hash and every corpus hash are in `manifest.json`.
The capture script and complete invocation log are retained in ignored scratch
`2026-10-18-services-check/`; no interpreter or original-source execution is a
native runtime fallback.

- `catalog.json`: all 88 original entries, all six fields, in insertion order.
  Fresh values exactly match the earlier independent `native-build-closure`
  catalog. Integer-valued original decimations and enum queue sizes are converted
  using the original generator's `int()` semantics; optional tuple ranges are
  represented as JSON arrays without changing their values.
- `generated.rs`: the exact original `generated()` output, including indentation,
  Python float representations, optional ranges and final newline. This was
  captured independently and then checked against the current candidate file.
- `schema.json`: all 154 original Event field names, retaining field order, plus
  the exact three missing service names. This is the full schema field set,
  including deprecated fields, not a reduced message schema or union-only view.
- `process-original.json`: 29 real original subprocess executions. These cover
  CLI/help/abbreviation/repetition/error cases, candidate file errors, newline
  conversion, six field mutations, insertion order/count, and emit-mode bypass.
  Raw stdout/stderr and status are retained. Only private copied-root and original
  repository path prefixes are normalized to `$ROOT` and `$REPO`; candidate input
  bytes are recorded in hexadecimal. `directory` and `missing` are explicit
  filesystem case markers.
- `format-original.json`: ten original generic generation controls, covering
  signed zero, thirds, subnormal/large/small/exponential-boundary floats, optional
  range formatting and integer truncation of queue size/decimation. Expectations
  come from the unchanged original function, not any Rust formatter.
- `schema-original.json`: seven executions of the unchanged original `main()`
  with controlled independent schema field dictionaries and real candidate files.
  These preserve missing-name semantics, deprecated/unrelated field behavior,
  exact assertion messages, catalog-before-schema ordering, and emit bypass.
  They are controlled policy captures, not claims of modified native schema
  compiler validation.

The existing generated Rust service catalog is always the candidate. The native
header/port generators include that file and therefore cannot serve as an
independent oracle for this check. Shared `services.py`, its remaining consumers
and original C++ header compilation test remain explicitly pending outside this
single-checker retirement.

A later independent CLI review found an exact diagnostic mismatch for explicit
flag values containing a quote/newline. `cli-repr-original.json` adds 18 fresh
unchanged-original subprocess cases, retaining all earlier fixture bytes. These
cover both quote choices, backslash, tabs/newlines/CR, nonprintable and printable
Unicode, invalid Unix argv bytes, short help and abbreviated long flags. The old
native executable matched six and failed twelve before repair; the full RED
comparisons are retained in ignored `cli-repr-red.json`.

`cli-repr-manifest.json` binds those cases to the same original checker/base
manifest and to the exact existing native mechanical string formatter and its
independent Unicode 15.0 printability table. Fresh exhaustive original Python
printability over all 1,114,112 code points confirmed that existing table's
contents. The new tool reuses that formatter through a visibility-only export;
no additional Unicode table, new dependency, interpreter, or candidate service
implementation is used. Copied-root regressions also mutate/remove each formatter
source pin independently.
