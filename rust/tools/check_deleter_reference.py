import argparse
import ast
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import types

from deleter_fixtures import scenarios

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "openpilot/system/loggerd"


def load_definitions(path, namespace, names=None):
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.Assign))
               and (names is None or getattr(node, "name", None) in names)]
  exec(compile(tree, str(path), "exec"), namespace)


class Source:
  def __init__(self, root):
    self.root = root
    self.deleted = None
    self.wait = None
    self.stopped = False
    xattr = runpy.run_path(str(SOURCE / "xattr_cache.py"))
    log = types.SimpleNamespace(info=lambda *_: None, exception=lambda *_: None)
    self.namespace = {"os": os, "cloudlog": log, "getxattr": xattr["getxattr"],
                      "Paths": types.SimpleNamespace(log_root=lambda: str(root)),
                      "shutil": types.SimpleNamespace(rmtree=self.remove),
                      "threading": types.SimpleNamespace(Event=object)}
    load_definitions(SOURCE / "uploader.py", self.namespace, {"get_directory_sort", "listdir_by_creation"})
    load_definitions(SOURCE / "deleter.py", self.namespace)

  def remove(self, path):
    shutil.rmtree(path)
    self.deleted = os.fsencode(os.path.basename(path)).hex()

  def is_set(self):
    return self.stopped

  def stop_wait(self, duration):
    self.wait = round(duration * 1000)
    self.stopped = True

  def run(self, command):
    try:
      if command["op"] == "preserved":
        directories = self.namespace["listdir_by_creation"](str(self.root))
        values = self.namespace["get_preserved_segments"](directories)
        return {"preserved": sorted(os.fsencode(value).hex() for value in values)}
      self.deleted = self.wait = None
      self.stopped = False
      self.namespace["get_available_bytes"] = lambda default: int(command["bytes"])
      self.namespace["get_available_percent"] = lambda default: float(command["percent"])
      event = types.SimpleNamespace(is_set=self.is_set, wait=self.stop_wait)
      self.namespace["deleter_thread"](event)
      return {"deleted": self.deleted, "wait_ms": self.wait}
    except OSError as error:
      return {"error": error.errno}


def snapshot(root):
  values = []
  for directory, dirs, files in os.walk(os.fsencode(root), followlinks=False):
    for name in dirs + files:
      path = os.path.join(directory, name)
      values.append((os.path.relpath(path, os.fsencode(root)).hex(), os.path.islink(path)))
  return sorted(values)


def action(root, step, outside):
  name = os.path.join(os.fsencode(root), step[1])
  if step[0] == "attr":
    os.setxattr(name, "user.preserve", step[2])
  elif step[0] == "lock":
    Path(os.fsdecode(os.path.join(name, b"held.lock"))).touch()
  elif step[0] == "symlink":
    os.symlink(outside, name)
  elif step[0] == "deny":
    os.chmod(name, 0)
  else:
    raise AssertionError(step)


def scenario(binary, output, name, directories, steps):
  work = output / name
  expected_root, actual_root, outside = [work / value for value in ("original", "rust", "outside")]
  for root in (expected_root, actual_root, outside):
    root.mkdir(parents=True)
  (outside / "untouched").write_bytes(b"keep")
  for root in (expected_root, actual_root):
    for directory in directories:
      path = os.path.join(os.fsencode(root), directory)
      os.mkdir(path)
      with open(os.path.join(path, b"rlog.zst"), "wb") as stream:
        stream.write(b"synthetic")
  original = Source(expected_root)
  trace = []
  with (work / "stderr.log").open("w") as errors:
    peer = subprocess.Popen([str(binary), str(actual_root)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors, text=True)
    try:
      for step in steps:
        if step[0] not in ("cycle", "preserved"):
          for root in (expected_root, actual_root):
            action(root, step, outside)
          continue
        command = {"op": step[0]}
        if step[0] == "cycle":
          command.update(bytes=str(step[1]), percent=step[2])
        peer.stdin.write(json.dumps(command) + "\n")
        peer.stdin.flush()
        actual = json.loads(peer.stdout.readline())
        expected = original.run(command)
        trace.append({"input": command, "expected": expected, "actual": actual})
        (work / "trace.json").write_text(json.dumps(trace, indent=2))
        assert actual == expected, (name, command, expected, actual)
        assert snapshot(expected_root) == snapshot(actual_root), name
        assert (outside / "untouched").read_bytes() == b"keep"
      peer.stdin.close()
      assert peer.wait(timeout=3) == 0
    finally:
      if peer.poll() is None:
        peer.kill()
        peer.wait()
      peer.stdout.close()
      for root in (expected_root, actual_root):
        for child in root.iterdir():
          if child.is_dir() and not child.is_symlink():
            child.chmod(0o700)
  return len(trace)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  rows = {name: scenario(args.binary.resolve(), args.output, name, directories, steps)
          for name, directories, steps in scenarios()}
  report = {"result": "pass", "scenarios": rows, "comparisons": sum(rows.values()),
            "oracle": "actual original deleter loop, ordering functions and xattr cache",
            "scope": "synthetic temporary filesystem; no vehicle or existing route data"}
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
