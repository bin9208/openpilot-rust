#!/usr/bin/env python3
"""Stage pinned wheel native libraries for Rust torqued; build tooling only."""

from __future__ import annotations
import argparse
import hashlib
import json
import tomllib
from pathlib import Path
import zipfile


def stage(wheel: Path, output: Path) -> None:
  lock = tomllib.loads((Path(__file__).resolve().parents[2] / "uv.lock").read_text())
  package = next(package for package in lock["package"] if package["name"] == "numpy")
  if package["version"] != "2.5.3":
    raise ValueError("torqued numerical ABI must be reviewed when the source NumPy lock changes")
  entry = next((entry for entry in package["wheels"] if entry["url"].rsplit("/", 1)[1] == wheel.name), None)
  digest = hashlib.sha256(wheel.read_bytes()).hexdigest()
  if entry is None or entry["hash"] != "sha256:" + digest:
    raise ValueError("numerical wheel must match an exact NumPy2.5.3 archive in uv.lock")
  output.mkdir(parents=True, exist_ok=False)
  files = []
  with zipfile.ZipFile(wheel) as archive:
    for name in archive.namelist():
      path = Path(name)
      if path.parts[0] == "numpy.libs" and path.name:
        data = archive.read(name)
        (output / path.name).write_bytes(data)
        files.append({"name": path.name, "sha256": hashlib.sha256(data).hexdigest()})
      elif ".dist-info/licenses/" in name and not name.endswith("/"):
        target = output / "licenses" / Path(name.split("/licenses/", 1)[1])
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(archive.read(name))
  (library,) = [entry["name"] for entry in files if entry["name"].startswith("libscipy_openblas64_")]
  manifest = {"format": 1, "numpy": "2.5.3", "abi": "scipy_dgesdd_64_", "library": library, "files": files}
  (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
  provenance = {
    "wheel": wheel.name,
    "sha256": hashlib.sha256(wheel.read_bytes()).hexdigest(),
    "source": "https://github.com/numpy/numpy/tree/v2.5.3",
    "runtime": "native shared libraries only; no Python imports",
  }
  (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
  print(json.dumps(provenance))


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--wheel", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  stage(args.wheel, args.output)
