"""Compare Rust traces against actual source Python classes, not copied formulas."""
import csv
import importlib.util
import io
import math
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("reference_filters", ROOT / "openpilot/common/filter_simple.py")
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)
trace = subprocess.check_output(["cargo", "run", "--quiet", "--locked", "--release", "--example", "filter_trace"], cwd=ROOT / "rust", text=True)
case = None
count = 0
max_error = 0.0
for row in csv.DictReader(io.StringIO(trace), delimiter="\t"):
    rc, dt, x = (float(row[k]) for k in ("rc", "dt", "x"))
    if row["case"] != case:
        assert int(row["step"]) == 0
        case = row["case"]
        initialized = row["initialized"] == "true"
        first = reference.FirstOrderFilter(2.0, rc, dt, initialized)
        bounce = reference.BounceFilter(2.0, rc, dt, initialized, bounce=2.0)
    first.update_alpha(rc)
    bounce.update_alpha(rc)
    for field, expected in (("first", first.update(x)), ("bounce", bounce.update(x))):
        actual = float(row[field])
        assert math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-12), (row, field, expected)
        max_error = max(max_error, abs(actual - expected))
        count += 1
assert count == 24000, count
print(f"PASS: {count} scalar outputs match source Python filters; max abs error={max_error:.3g}")
