The complete unmodified check_selfdrive_helpers.py main generated all 10,241
fixed cases and source results on CPython 3.12.15, NumPy 2.5.3 and pycapnp
2.1.0 before any replacement candidate. The capture intercepts subprocess.run
only after original PoseCalibrator/ExcessiveActuationCheck/camera computations.
Its peer forwards independent source responses with source NaN mapped to null,
as the original comparator requires; it contributes no expected policy values.

corpus.json.gz stores exact input.jsonl/source.json bytes, original expected
responses as JSON lines, NaN-to-null peer responses, and original report/stdout
templates. Only the candidate SHA256 is filled after successful comparison.
The six core fixture files independently regenerate byte for byte.

failures.json records 17 candidate/subprocess outcomes. io-failures.json records
four post-child write/print faults. These failure captures execute the untouched
original main AST suffix beginning at subprocess.run with independently computed
source locals. pre-child-failures.json separately captures original main input/
source write failures; these two boundaries are documented but not injected by
the Rust Runner seam. comparisons.json records 25 original comparator type,
tolerance, nonfinite and iterable-admission cases.

Only this fixed checker is retired. All 15 generic source/schema files remain
mandatory live pins; selected NumPy/pycapnp uv.lock records also remain pinned.
No native checker executes Python or changes pose/calibration/actuation/camera
policy. Original argparse wording is not an exact diagnostic-byte contract;
CLI status and side-effect behavior are tested.
