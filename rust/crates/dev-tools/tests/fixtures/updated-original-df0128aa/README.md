# Independent updater controller observations

These data files came from the unchanged Python `rust/tools/check_updated.py`
lane in [Actions run 39536560193, job 119014447037](https://github.com/bin9208/openpilot-rust/actions/runs/39536560193/job/119014447037).
The diagnostic workflow head was `ec7d3e2fec386cae36855ed2cff6f968176d5837`;
it explicitly checked out source `df0128aa055f2ff5899c99791f73bb00b52f8ce0`.
Artifact `12318550611` has ZIP SHA256
`f4ce53cad77e64db350cba0ebfd9ae975228d5556f48c6c6a98e63def230fbd9`.
The artifact's `source-commit.txt`, `source-inputs.sha256`,
`executables.sha256`, original controller directory, and 11 CLI receipts
provide the capture provenance. No Python source bytes are included here.

The five named JSON files are exact byte copies of
`original-controller/<name>-source.json`: the full normalized original
snapshot, ordered command, AGNOS, and Params observations. Their SHA256 values
are checked independently in `updated_check.rs`. The original
source snapshots total 5, 3, 2, 2, and 7; their command lists have 144, 59,
53, 53, and 26 entries. Only the exact obsolete staging removal commands may
be removed by the native controller. Integral wait numeric representation is
handled only in snapshot `wait` fields.

`raw-original/` retains the ten unmodified original source-process stdout and
stderr files for those scenarios. Its `SHA256SUMS` lists every file and has
SHA256 `c7100b2d3b706cc76e7c115835707ecbde46ab3c2a6051e17ba4b731fd44e29b`.
These raw captures contain the original run's temporary paths, so the native
comparison uses the complete normalized JSON above while keeping raw evidence
available for audit.

`cli-original.json` contains the original half of each of the 11 retained CLI
receipts, in the original case order, with raw stdout/stderr hex, status, argv,
and source/interpreter hashes. It was projected from the artifact receipts;
native outputs were deliberately excluded. The projection SHA256 is
`a9806418ef44248e24a494f37ec387195f6d87996e5de7278c0fc8205cf8372f`.
The original Python checker was retired after its same-ELF absence proof.
`updated_fixtures.py` remains for the original daemon checker until that
controller's independent comparison and absence proof are accepted.

The controller's `--root` option changes admission only for the three
`rust/tools/check_updated.py`, `updated_source.py`, and `updated_fixtures.py`
paths. The five unchanged runtime/original dependency pins are still read from
the actual checkout. The first absence cloud run retains full `strace` files
to inventory Git, launcher, fixture, checker, candidate, and any real native
helper executions. The accepted updater proof uses its complete observed
executable/argv allowlist; preparation for the helper's own retirement now
requires its absence in the private root while the checkout helper is
exact-hash-pinned when present.
