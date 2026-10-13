# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys


def main() -> None:
  native, cases, output = (Path(value).resolve() for value in sys.argv[1:])
  output.mkdir(parents=True, exist_ok=True)
  source = Path(__file__).with_name("cluster_linux_stats_source.py")
  records = []
  for case in sorted(cases.glob("*.json")):
    record = {"case": case.stem}
    for name, command in (("source", [sys.executable, str(source)]), ("native", [str(native)])):
      destination = output / case.stem / name
      command += [str(case), str(destination)]
      result = subprocess.run(command, capture_output=True, text=True, timeout=20, check=False)
      (output / f"{case.stem}-{name}.stdout").write_text(result.stdout)
      (output / f"{case.stem}-{name}.stderr").write_text(result.stderr)
      record[name] = {"command": command, "exit_code": result.returncode}
      try:
        record[name]["value"] = json.loads(result.stdout.strip().splitlines()[-1])
      except (ValueError, IndexError):
        record[name]["value"] = None
    record["match"] = record["source"]["exit_code"] == record["native"]["exit_code"] == 0 and record["source"]["value"] == record["native"]["value"]
    records.append(record)
  original = source.parent.parent.parent / "openpilot/selfdrive/carrot/cluster/cluster_system_monitor.py"
  result = {"native_sha256": hashlib.sha256(native.read_bytes()).hexdigest(), "source_sha256": hashlib.sha256(original.read_bytes()).hexdigest(), "adapter_sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "records": records, "passed": sum(record["match"] for record in records), "total": len(records)}
  (output / "result.json").write_text(json.dumps(result, indent=2))
  print(json.dumps({"passed": result["passed"], "total": result["total"]}))
  if result["passed"] != result["total"]:
    raise SystemExit(1)


if __name__ == "__main__":
  main()
