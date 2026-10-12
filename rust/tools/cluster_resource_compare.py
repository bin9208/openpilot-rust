from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys


def main():
  root, reference, binary, output = (Path(value).resolve() for value in sys.argv[1:])
  command = [sys.executable, str(root / "rust/tools/cluster_navigation_render_compare.py"), str(root), str(reference), str(binary), str(output)]
  result = subprocess.run(command, check=False)
  source = json.loads((reference / "source-result.json").read_text())["captures"]
  native = json.loads((output / "native-result.json").read_text())["captures"]
  differences = []
  for expected, actual in zip(source, native, strict=True):
    assert expected["name"] == actual["name"]
    fields = ("reuse_previous", "profile_names", "fitted")
    failed = {key: {"source": expected.get(key), "native": actual.get(key)} for key in fields if expected.get(key) != actual.get(key)}
    if failed:
      differences.append(dict(name=expected["name"], fields=failed))
  (output / "events.json").write_text(json.dumps(dict(cases=len(source), differences=differences), indent=2))
  print(json.dumps(dict(event_differences=len(differences), first=differences[:3])))
  raise SystemExit(result.returncode or bool(differences))


if __name__ == "__main__":
  main()
