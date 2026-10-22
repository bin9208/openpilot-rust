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
  if args.output.is_symlink():
    raise RuntimeError("SDK output escapes destination: output is a symlink")
  args.output.mkdir(parents=True, exist_ok=True)
  output_root = args.output.resolve()
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

  files, directory_aliases = [], []
  directories = {}

  def image_path(path):
    if not path.startswith("/") or any(c in path for c in '\\"\0\n\r'):
      raise RuntimeError(f"unsafe image path: {path!r}")
    return "/" + posixpath.normpath(path).lstrip("/")

  def destination(path):
    output = args.output.joinpath(*PurePosixPath(image_path(path)).parts[1:])
    if not output.resolve().is_relative_to(output_root):
      raise RuntimeError(f"SDK output escapes destination: {path}")
    return output

  def resolve(path):
    path = image_path(path)
    seen = []
    while True:
      if path in seen or len(seen) >= 64:
        raise RuntimeError(f"image symlink loop or depth limit: {path}")
      seen.append(path)
      status = debug('stat "' + path + '"')
      if "Type: regular" in status or "Type: directory" in status:
        return path, status, seen
      link = re.search(r'Fast link dest: "([^"\n]+)"', status)
      if link is None:
        raise RuntimeError(f"missing image file {path}: {status}")
      path = image_path(posixpath.join(posixpath.dirname(path), link[1]))

  def extract(requested, resolved=None):
    requested = image_path(requested)
    path, status, seen = resolved or resolve(requested)
    if "Type: regular" not in status:
      raise RuntimeError(f"missing image file {path}: {status}")
    output = destination(requested)
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name(output.name + ".extracting")
    temporary.unlink(missing_ok=True)
    debug(f'dump "{path}" "{temporary}"')
    expected_size = re.search(r"\bSize:\s+(\d+)", status)
    if not expected_size or not temporary.is_file() or temporary.stat().st_size != int(expected_size[1]):
      raise RuntimeError(f"bad extracted file size: {requested}")
    temporary.replace(output)
    files.append({"requested": requested, "image_path": path, "alias_chain": seen, "output": str(output),
                  "sha256": hashlib.sha256(output.read_bytes()).hexdigest()})
    return output

  def walk(requested, source=None, ancestors=()):
    requested = image_path(requested)
    path, status, seen = resolve(source or requested)
    if "Type: directory" not in status:
      return extract(requested, (path, status, seen))
    if path in ancestors:
      raise RuntimeError(f"image directory symlink loop: {path}")
    if len(ancestors) >= 128 or len(directories) >= 100000:
      raise RuntimeError("image directory traversal limit")
    output = destination(requested)
    if path not in directories:
      # A child-first root may already have created this canonical ancestor.
      # Materialize at image identity, never at an alias that could later point
      # back through those existing child directories.
      target = destination(path)
      directories[path] = target
      target.mkdir(parents=True, exist_ok=True)
      for line in debug('ls -p "' + path + '"').splitlines():
        fields = line.split("/")
        if len(fields) < 7 or fields[5] in (".", ".."):
          continue
        # Inspect symlinks as well as directories at the image boundary.
        child = path.rstrip("/") + "/" + fields[5]
        walk(child, child, ancestors + (path,))
    target = directories[path]
    if output.resolve() != target.resolve():
      # Image aliases are relative links to one output-contained canonical tree.
      output.parent.mkdir(parents=True, exist_ok=True)
      output.symlink_to(posixpath.relpath(target, output.parent), target_is_directory=True)
      directory_aliases.append({"requested": requested, "image_path": path, "alias_chain": seen,
                                "output": str(output), "target": str(target)})

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
  record = {"image": str(args.image), "raw_sha256": digest, "files": files, "directory_aliases": directory_aliases}
  args.record.parent.mkdir(parents=True, exist_ok=True)
  args.record.write_text(json.dumps(record, indent=2) + "\n")
  print(json.dumps({"files": len(files), "libraries": len(done), "raw_sha256": digest}), flush=True)


if __name__ == "__main__":
  main()
