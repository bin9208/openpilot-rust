import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import zipfile

ROOT = Path(__file__).resolve().parents[2]
WHEELS = {
  "shapely-2.1.2-cp312-cp312-manylinux2014_x86_64.manylinux_2_17_x86_64.whl":
    ("x86_64", "1e7d4d7ad262a48bb44277ca12c7c78cb1b0f56b32c10734ec9a1d30c0b0c54b", "native-dependencies.json", 62),
  "shapely-2.1.2-cp312-cp312-manylinux2014_aarch64.manylinux_2_17_aarch64.whl":
    ("aarch64", "0bd308103340030feef6c111d3eb98d50dc13feea33affc8a6f9fa549e9458a3", "geos-aarch64.json", 183),
}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--wheel", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  architecture, digest, manifest_file, machine = WHEELS[args.wheel.name]
  assert hashlib.sha256(args.wheel.read_bytes()).hexdigest() == digest, "pinned Shapely wheel hash"
  manifest = json.loads((ROOT / "rust/crates/carrot-man" / manifest_file).read_text())
  files = {}
  with zipfile.ZipFile(args.wheel) as wheel:
    for library in manifest["libraries"]:
      content = wheel.read("shapely.libs/" + library["name"])
      assert hashlib.sha256(content).hexdigest() == library["sha256"], "pinned GEOS library hash"
      assert content[:6] == b"\x7fELF\x02\x01" and struct.unpack("<H", content[18:20])[0] == machine, "pinned GEOS ELF architecture"
      files[library["name"]] = content
    for name in ("LICENSE_GEOS", "LICENSE.txt"):
      files[name] = wheel.read("shapely-2.1.2.dist-info/licenses/" + name)
  args.output.parent.mkdir(parents=True, exist_ok=True)
  free = shutil.disk_usage(args.output.parent).free
  assert free >= 25 * 1024**3 + sum(map(len, files.values())) + 1024**2, "25 GiB plus artifact growth floor"
  args.output.mkdir(exist_ok=False)
  for name, content in files.items():
    (args.output / name).write_bytes(content)
  (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
  library = next(item["name"] for item in manifest["libraries"] if item["name"].startswith("libgeos_c-"))
  (args.output / "environment.txt").write_text(f"CARROT_GEOS_LIBRARY={args.output.resolve() / library}\nCARROT_GEOS_MANIFEST={args.output.resolve() / 'manifest.json'}\n")
  (args.output / "provenance.json").write_text(json.dumps(dict(architecture=architecture, wheel=args.wheel.name, wheelSha256=digest,
    source=f"https://pypi.org/project/shapely/2.1.2/#files"), indent=2) + "\n")
  print(f"verified Shapely2.1.2 GEOS3.13.1 {architecture}")


if __name__ == "__main__":
  main()
