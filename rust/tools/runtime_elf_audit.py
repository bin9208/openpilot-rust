from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from validate_runtime_ffmpeg import elf


def audit(roots: list[Path], libraries: list[Path], machine: int = 183) -> dict:
  objects = {}
  providers = {}
  pending = [path.resolve(strict=True) for path in roots]
  while pending:
    path = pending.pop()
    if path in objects:
      continue
    with path.open("rb") as stream:
      header = stream.read(64)
    if header[:6] != b"\x7fELF\x02\x01" or int.from_bytes(header[18:20], "little") != machine:
      raise RuntimeError(f"unexpected ELF architecture: {path}")
    objects[path] = elf(path)
    for needed in objects[path]["needed"]:
      found = next((folder / needed for folder in libraries if (folder / needed).is_file()), None)
      if found is None:
        raise RuntimeError(f"missing provider {needed} required by {path}")
      providers[needed] = found.resolve(strict=True)
      pending.append(providers[needed])
  results = []
  for root in roots:
    closure = set()
    pending = [root.resolve(strict=True)]
    while pending:
      path = pending.pop()
      if path in closure:
        continue
      closure.add(path)
      pending.extend(providers[needed] for needed in objects[path]["needed"])
    exports = set().union(*(objects[path]["exports"] for path in closure))
    failures = []
    for path in sorted(closure):
      obj = objects[path]
      missing_versions = {provider: sorted(set(versions) - set(objects[providers[provider]]["definitions"]))
                          for provider, versions in obj["needs"].items()}
      missing_versions = {key: value for key, value in missing_versions.items() if value}
      missing_symbols = sorted(obj["imports"] - exports)
      if missing_versions or missing_symbols:
        failures.append({"path": str(path), "missing_versions": missing_versions, "missing_strong_symbols": missing_symbols})
    results.append({"root": str(root.resolve()), "closure": list(map(str, sorted(closure))), "failures": failures, "compatible": not failures})
  return {"roots": results, "compatible": all(row["compatible"] for row in results),
          "objects": [{"path": str(path), "needed": obj["needed"], "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                      for path, obj in sorted(objects.items())],
          "scope": "static ELF64 little-endian architecture, per-root DT_NEEDED closure, version and strong-symbol resolution; no hardware execution"}


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, action="append", required=True)
  cli.add_argument("--library", type=Path, action="append", required=True)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  report = audit(args.root, args.library)
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps({"compatible": report["compatible"], "objects": len(report["objects"]), "roots": len(report["roots"])}))
  if not report["compatible"]:
    raise RuntimeError("native library closure is incompatible")


if __name__ == "__main__":
  main()
