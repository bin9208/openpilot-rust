#!/usr/bin/env python3
import argparse
import hashlib
import json
from pathlib import Path


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("source", type=Path)
  parser.add_argument("native", type=Path)
  parser.add_argument("binary", type=Path)
  parser.add_argument("output", type=Path)
  args = parser.parse_args()
  source = json.loads((args.source / "source-result.json").read_text())
  native = json.loads((args.native / "native-result.json").read_text())
  expected = {row["name"]: row for row in source["records"]}
  actual = {row["name"]: row for row in native["records"]}
  records, errors = [], []
  for name, row in expected.items():
    if name not in actual:
      errors.append({"name": name, "error": "missing native case"})
      continue
    source_bytes = Path(row["path"]).read_bytes()
    native_bytes = Path(actual[name]["path"]).read_bytes()
    match = source_bytes == native_bytes
    result = {
      "name": name,
      "bytes": len(source_bytes),
      "match": match,
      "source_sha256": hashlib.sha256(source_bytes).hexdigest(),
      "native_sha256": hashlib.sha256(native_bytes).hexdigest(),
    }
    records.append(result)
    if not match:
      differences = [index for index, pair in enumerate(zip(source_bytes, native_bytes, strict=False)) if pair[0] != pair[1]]
      errors.append(
        {
          "name": name,
          "source_bytes": len(source_bytes),
          "native_bytes": len(native_bytes),
          "differing_bytes": len(differences),
          "first": [[index, source_bytes[index], native_bytes[index]] for index in differences[:32]],
        }
      )
  if expected.keys() != actual.keys():
    errors.append({"error": "case inventory differs"})
  result = {
    "cases": len(expected),
    "matched": sum(row["match"] for row in records),
    "source_shader_sha256": source["source_sha256"],
    "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    "records": records,
    "errors": errors,
  }
  args.output.write_text(json.dumps(result, indent=2))
  print(json.dumps({"cases": result["cases"], "matched": result["matched"], "failures": len(errors)}))
  raise SystemExit(bool(errors))


if __name__ == "__main__":
  main()
