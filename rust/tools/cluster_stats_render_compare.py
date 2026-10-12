# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow"]
# ///
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys
from PIL import Image, ImageChops


def main() -> None:
  root, reference, native, output = (Path(value).resolve() for value in sys.argv[1:])
  output.mkdir(parents=True, exist_ok=True)
  command = [str(native), str(root), str(reference / "input.json"), str(output)]
  digest = hashlib.sha256(native.read_bytes()).hexdigest()
  with (output / "native.log").open("wb") as log:
    subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=120, check=True)
  source_records = json.loads((reference / "result.json").read_text())
  native_records = json.loads((output / "result.json").read_text())
  differences = []
  images = 0
  errors = 0
  for expected, actual in zip(source_records, native_records, strict=True):
    assert (expected["name"], expected["warm"]) == (actual["name"], actual["warm"])
    if expected["error"] != actual["error"]:
      differences.append(dict(name=expected["name"], error=[expected["error"], actual["error"]]))
    if expected["path"] is not None:
      assert actual["path"] is not None
      with Image.open(expected["path"]) as source_image, Image.open(actual["path"]) as native_image:
        assert source_image.size == native_image.size
        difference = ImageChops.difference(source_image.convert("RGBA"), native_image.convert("RGBA"))
        if any(band.getbbox() is not None for band in difference.split()):
          differences.append(dict(name=expected["name"], warm=expected["warm"], pixels="differ"))
      images += 1
    else:
      assert actual["path"] is None
      errors += 1
  assert digest == hashlib.sha256(native.read_bytes()).hexdigest()
  result = dict(command=command, native_sha256=digest, observations=len(source_records), images=images, errors=errors, differences=differences)
  (output / "comparison.json").write_text(json.dumps(result, indent=2))
  print(json.dumps(dict(images=images, errors=errors, differences=len(differences))))
  if differences:
    raise SystemExit(1)


if __name__ == "__main__":
  main()
