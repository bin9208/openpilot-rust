from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import posixpath
import re
import shutil
import subprocess


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--image", type=Path, required=True)
  cli.add_argument("--identity", type=Path, required=True)
  cli.add_argument("--output", type=Path, required=True)
  cli.add_argument("--record", type=Path, required=True)
  cli.add_argument("--tree", action="append", default=[])
  cli.add_argument("--file", action="append", default=[])
  cli.add_argument("--library", action="append", default=[])
  args = cli.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  if shutil.disk_usage(args.output).free < 27 * 1024**3:
    raise RuntimeError("SDK extraction needs 25 GiB reserve plus 2 GiB estimated growth")
  identity = json.loads(args.identity.read_text())
  if args.image.stat().st_size != identity["raw_size"]:
    raise RuntimeError("pinned image size differs")
  with args.image.open("rb") as stream:
    digest = hashlib.file_digest(stream, "sha256").hexdigest()
  if digest != identity["raw_sha256"]:
    raise RuntimeError("pinned image digest differs")

  def debug(command):
    result = subprocess.run(["/usr/sbin/debugfs", "-R", command, str(args.image)], check=True, capture_output=True, text=True)
    return result.stdout

  files = []

  def extract(path):
    requested = path
    seen = []
    while True:
      if path in seen:
        raise RuntimeError(f"image symlink loop: {path}")
      seen.append(path)
      status = debug("stat " + path)
      if "Type: regular" in status:
        break
      link = re.search(r'Fast link dest: "([^"\n]+)"', status)
      if link is None:
        raise RuntimeError(f"missing image file {path}: {status}")
      path = posixpath.normpath(posixpath.join(posixpath.dirname(path), link[1]))
    output = args.output.joinpath(*PurePosixPath(requested).parts[1:])
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name(output.name + ".extracting")
    temporary.unlink(missing_ok=True)
    debug(f"dump {path} {temporary}")
    expected_size = re.search(r"\bSize:\s+(\d+)", status)
    if not expected_size or not temporary.is_file() or temporary.stat().st_size != int(expected_size[1]):
      raise RuntimeError(f"bad extracted file size: {requested}")
    temporary.replace(output)
    files.append({"requested": requested, "image_path": path, "alias_chain": seen, "output": str(output),
                  "sha256": hashlib.sha256(output.read_bytes()).hexdigest()})
    return output

  def walk(path):
    for line in debug("ls -p " + path).splitlines():
      fields = line.split("/")
      if len(fields) < 7 or fields[5] in (".", ".."):
        continue
      child = path.rstrip("/") + "/" + fields[5]
      if fields[2].startswith("04"):
        walk(child)
      else:
        extract(child)

  for tree in args.tree:
    walk(tree)
  for path in args.file:
    extract(path)
  pending, done = list(args.library), set()
  while pending:
    name = pending.pop()
    if name in done:
      continue
    done.add(name)
    output = extract("/usr/lib/aarch64-linux-gnu/" + name)
    dynamic = subprocess.run(["readelf", "--wide", "-d", str(output)], check=True, capture_output=True, text=True).stdout
    pending.extend(re.findall(r"\((?:NEEDED|SONAME)\).*?\[(.*?)\]", dynamic))
  record = {"image": str(args.image), "raw_sha256": digest, "files": files}
  args.record.parent.mkdir(parents=True, exist_ok=True)
  args.record.write_text(json.dumps(record, indent=2) + "\n")
  print(json.dumps({"files": len(files), "libraries": len(done), "raw_sha256": digest}), flush=True)


if __name__ == "__main__":
  main()
