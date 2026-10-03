from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import tempfile


def source(binding: Path, root: Path, value: str) -> None:
    from card_runtime_source import load_binding
    load_binding(binding)
    from openpilot.common.params import Params
    settings = Params(str(root))
    Path(settings.get_param_path("SpeedFromPCM")).write_bytes(value.encode())
    print(settings.get_int("SpeedFromPCM"), flush=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binding", type=Path, required=True)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--numerics", type=Path)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--source-root", type=Path)
    parser.add_argument("--value")
    args = parser.parse_args()
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    if args.source_root is not None:
        source(args.binding, args.source_root, args.value)
        return
    from can_source import ROOT
    from card_qa.mazda.scenarios import cases
    assert args.binary is not None and args.numerics is not None and args.evidence is not None
    args.evidence.mkdir(parents=True, exist_ok=True)
    provenance = json.loads(args.binding.with_name("provenance.json").read_text())
    assert hashlib.sha256(args.binding.read_bytes()).hexdigest() == provenance["module_sha256"]
    for name, digest in provenance["sources"].items():
        assert hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == digest, name
    template = next(case for case in cases() if case["op"] == "runtime")
    rows = []
    values = (("1", 1), (" +1suffix", 1), ("0xFF", 0), (" -2tail", -2), ("bad", None), ("2147483648", None))
    for index, (value, expected) in enumerate(values):
        with tempfile.TemporaryDirectory(prefix="mazda-param-source-") as temporary:
            environment = {**os.environ, "OPENPILOT_PREFIX": f"mazda-int-{os.getpid()}-{index}"}
            command = [sys.executable, str(Path(__file__).resolve()), "--binding", str(args.binding), "--source-root", temporary, "--value", value]
            observed = subprocess.run(command, env=environment, text=True, capture_output=True, timeout=10, check=False)
        row = dict(value=value, source_exit=observed.returncode, source_stdout=observed.stdout, source_stderr=observed.stderr, source_command=command)
        if expected is not None:
            assert observed.returncode == 0 and observed.stdout.strip() == str(expected), row
        else:
            assert observed.returncode == -6 and "stoi" in observed.stderr, row
            case = copy.deepcopy(template)
            case["steps"] = case["steps"][:1]
            case["steps"][0]["settings"]["SpeedFromPCM"] = value
            output = args.evidence / f"invalid-{index}.json"
            output.unlink(missing_ok=True)
            command = [str(args.binary.resolve()), str(output.resolve()), str(ROOT / "opendbc_repo/opendbc/dbc"), str(ROOT / "opendbc_repo/opendbc/car/torque_data"), str(args.numerics)]
            native = subprocess.run(command, input=json.dumps([case]), text=True, capture_output=True, timeout=10, check=False)
            assert native.returncode == 1 and "Numeric" in native.stderr and not output.exists(), native
            row.update(native_exit=native.returncode, native_stderr=native.stderr, native_stdout=native.stdout, native_command=command, emitted_result=False)
        rows.append(row)
    report = dict(status="pass", scenarios=rows, binding_sha256=provenance["module_sha256"], binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(), scope="Actual unchanged Cython/C++ Params.get_int; valid prefix parses and source SIGABRT versus typed native fatal outcome before serialized CAN result; no device")
    (args.evidence / "result.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
