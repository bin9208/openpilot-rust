Independent original card/RadarCAN DBC packaging captures, before native API code.

`catalog-original.json` preserves complete asset names/hashes and original manifests
from actual unchanged CLI runs. Card produced 97 assets / 4,830,112 bytes; RadarCAN
produced 103 assets / 5,010,641 bytes. Six RadarCAN assets are absent from card. Two
shared Hyundai corner-radar generated files have DIFFERENT bytes: card copies the
checked-in versions while RadarCAN regenerates them. A single all-generated catalog
is therefore not a valid replacement for both consumers.

`failures-original.json` preserves nine actual original API errors/effects and
fourteen actual CLI results. Existing directory/file markers remain untouched.
Card refuses an existing destination with FileExistsError and no errno, while
RadarCAN mkdir raises errno 17. Missing parents fail before publishing. Missing
includes cause card's generation subprocess to fail without a published directory;
RadarCAN has already created its destination and leaves it empty. Card rejects
extra CLI arguments; RadarCAN ignores them. Raw original stdout/stderr and full
invocations remain in ignored consumer captures; normalized templates name only
transient scratch roots/staging paths. No expected native artifact is used.

`provenance.json` pins both packagers, the three original card-asset tests, original
Python generator inputs and archived lazy/SCons caller versions. The original lazy
facade actually returned 45 generated outputs / 992,535 bytes, reused the SAME mutable
dictionary, left its cache unset on real generation failure and retried successfully
after the private input was restored. Both SCons files were inspected only, never
executed or built. Their archived old pins intentionally do not require migrated
current callers to retain old source bytes.

Native metadata contract proposed by the API tests: retain original manifest field
types and every actual input identity. Add an explicit `generator_provenance` object
with `implementation="native"`, native `source_sha256`, executing-ELF
`executable_sha256`, and historical `reference_python_sha256`. Missing retired
Python files must not be mislabeled as current inputs or silently disappear from
the evidence. Current changed SCons input gets its CURRENT hash; its old hash is
separately pinned by the archived capture. This is an intentional provenance
migration, not a claim that original and native generator execution are identical.
Native source provenance belongs to the executed generator implementation, which
may be outside a temporary repository containing only packaging DBC inputs.

Original source scripts and complete raw receipts are in ignored
`.analysis/scratch/2026-10-18-dbc-generator/consumers/`. No compiler, C++/JIT, install,
native expected-value generation or project-source mutation was used for captures.
The `dbc_assets` API/tests are prepared for missing-module RED only; there is no
native assets implementation or retirement claim yet.

`paths-original.json` adds eight actual unchanged API controls: dangling destination
symlink, symlink to an existing directory, non-UTF8 destination and non-UTF8 source
repository, for both consumers. Card follows Path.exists for its early refusal;
a dangling link reaches the final rename and fails NotADirectoryError/errno 20,
leaving the link intact. RadarCAN mkdir refuses the same link with errno 17.
Both raw Unix path success cases preserve their respective complete asset catalogs.
Immediate destination-parent statvfs errors must not be converted into nearest-
existing-ancestor success. The original consumers have no default destination;
relative destination arguments are interpreted in the caller's working directory.
Native RadarCAN keeps its first destination argument and ignores old extra args;
OPENPILOT_RUST_ROOT selects the repository, while its library API accepts sourceDBC.
Card retains exactly two CLI arguments. No --source option is required for RadarCAN.

The proposed native CLI tests use the same catalogs/effects and a documented output
framing transition. Card prints `Prepared 97 DBC assets with native Rust generator`;
RadarCAN prints `Prepared 103 original RadarCAN DBCs with native Rust generator`,
followed by a newline. This replaces the old card Python-used banner truthfully.
Missing arguments keep exit 1 and empty stdout, with usage naming the native
command; RadarCAN's original missing-argv traceback is not claimed to be retained.
All non-usage failures retain the original error-class label, filesystem effects
and API errno checks. RadarCAN still ignores every argument after destination,
even text such as `--source`; OPENPILOT_RUST_ROOT selects its repository privately.
Actual CLI tests remain uncompiled until parent registration and the genuine RED.
