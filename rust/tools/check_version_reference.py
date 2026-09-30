#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pyzmq==27.1.0"]
# ///
# How to run: uv run --no-project --python 3.12 rust/tools/check_version_reference.py BINARY [RUNNER ARGS...]
"""Compare the unchanged version/Git helpers with a persistent native probe and real Git fixtures."""
from __future__ import annotations

import ast
from contextlib import contextmanager
from dataclasses import asdict
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import traceback
from types import ModuleType
import uuid

import zmq

ROOT = Path(__file__).resolve().parents[2]
SOURCES = ["openpilot/system/version.py", "openpilot/common/git.py", "openpilot/common/utils.py"]


def capture(function):
  try:
    return {"value": function()}
  except (AttributeError, TypeError, IndexError, KeyError, OSError, UnicodeError, ValueError, subprocess.CalledProcessError) as error:
    return {"error": type(error).__name__}
  except Exception as error:  # noqa: BROAD_EXCEPT_OK - protocol records the source's explicit generic metadata error
    if str(error) != "invalid build metadata":
      raise
    return {"error": type(error).__name__}


def load_source():
  """Load original files unchanged; only optional import edges are isolated."""
  for name in ["openpilot", "openpilot.common", "openpilot.system"]:
    module = ModuleType(name)
    module.__path__ = []
    sys.modules[name] = module
  utils = ModuleType("openpilot.common.utils")
  utils.subprocess = subprocess
  tree = ast.parse((ROOT / SOURCES[2]).read_text())
  functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in {"run_cmd", "run_cmd_default"}]
  assert len(functions) == 2
  exec(compile(ast.Module(body=functions, type_ignores=[]), str(ROOT / SOURCES[2]), "exec"), utils.__dict__)
  sys.modules[utils.__name__] = utils
  basedir = ModuleType("openpilot.common.basedir")
  basedir.BASEDIR = str(Path.cwd())
  sys.modules[basedir.__name__] = basedir
  logs = []
  swaglog = ModuleType("openpilot.common.swaglog")

  class Logger:
    def exception(self, message):
      logs.append({"msg": message, "levelnum": 40, "exc_info": traceback.format_exc()})

  swaglog.cloudlog = Logger()
  sys.modules[swaglog.__name__] = swaglog
  loaded = []
  for name, path in [("openpilot.common.git", SOURCES[1]), ("openpilot.system.version", SOURCES[0])]:
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    loaded.append(module)
  return *loaded, logs


def source_worker() -> None:
  git, version, logs = load_source()

  def metadata(build):
    result = asdict(build)
    result["properties"] = {
      name: capture(lambda name=name: getattr(owner, name))
      for owner, names in [(build.openpilot, ["short_version", "git_normalized_origin", "comma_remote"]),
                           (build, ["tested_channel", "release_channel", "canonical", "ui_description"])]
      for name in names
    }
    return result

  def call(request):
    match request["op"]:
      case "metadata": return metadata(version.get_build_metadata(request["path"]))
      case "from_json": return metadata(version.build_metadata_from_dict(json.loads(request["source"])))
      case "git":
        helper = getattr(git, "get_" + request["method"])
        args = []
        if "cwd" in request:
          args.append(request["cwd"])
        if request["method"] in {"commit", "commit_date"}:
          if not args:
            args.append(None)
          args.append(request.get("revision", "HEAD"))
        return helper(*args)
      case "version": return version.get_version(request["path"])
      case "release_notes": return version.get_release_notes(request["path"])
      case "dirty": return version.is_dirty(request["path"])
      case "prebuilt": return version.is_prebuilt(request["path"])
      case "chdir": return os.chdir(request["path"])
      case _: raise AssertionError(request)

  for line in sys.stdin:
    logs.clear()
    response = capture(lambda: call(json.loads(line)))
    response["logs"] = logs
    print(json.dumps(response), flush=True)


def normal(value):
  match value:
    case float() if math.isnan(value): return "<NaN>"
    case list(): return [normal(item) for item in value]
    case dict(): return {key: normal(item) for key, item in value.items()}
    case _: return value


def git(path: Path, *arguments: str) -> bytes:
  return subprocess.check_output(["git", *arguments], cwd=path, stderr=subprocess.PIPE)


def fixture(path: Path, tracking: bool = True, origin: bool = True) -> Path:
  path.mkdir()
  git(path, "init", "-b", "main")
  git(path, "config", "user.name", "Version fixture")
  git(path, "config", "user.email", "version@example.invalid")
  git(path, "config", "commit.gpgsign", "false")
  git(path, "config", "core.hooksPath", "/dev/null")
  (path / "openpilot/common").mkdir(parents=True)
  (path / "openpilot/common/version.h").write_text('#define COMMA_VERSION "1.2.3-test"\n')
  (path / "RELEASES.md").write_text("First line\nsecond line\n\nnext release\n")
  (path / "tracked").write_text("original\n")
  git(path, "add", ".")
  git(path, "commit", "-m", "fixture")
  if origin:
    git(path, "remote", "add", "origin", "git@github.com:commaai/openpilot.git")
  if tracking:
    assert origin
    git(path, "update-ref", "refs/remotes/origin/main", "HEAD")
    git(path, "branch", "--set-upstream-to=origin/main")
  return path


class Pair:
  def __init__(self, binary: list[str], cwd: Path, directory: Path, label: str, environment: dict[str, str], evidence):
    self.evidence = evidence
    self.label = label
    self.context = zmq.Context()
    self.socket = self.context.socket(zmq.PULL)
    self.socket.setsockopt(zmq.RCVTIMEO, 2000)
    prefix = "version-" + uuid.uuid4().hex
    self.endpoint_path = Path("/tmp/logmessage" + prefix)
    self.socket.bind("ipc://" + str(self.endpoint_path))
    env = {**environment, "OPENPILOT_PREFIX": prefix}
    self.errors = [(directory / f"{label}-{kind}.stderr").open("w") for kind in ["source", "native"]]
    commands = [[sys.executable, str(Path(__file__).resolve()), "--source"], binary]
    self.processes = [subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=error, text=True) for command, error in zip(commands, self.errors, strict=True)]
    self.count = 0

  def call(self, op: str, **fields):
    request = {"op": op, **fields}
    results = []
    for process in self.processes:
      assert process.stdin is not None and process.stdout is not None
      process.stdin.write(json.dumps(request) + "\n")
      process.stdin.flush()
      assert select.select([process.stdout], [], [], 10)[0], (self.label, request, "probe timeout")
      line = process.stdout.readline()
      assert line, (self.label, request, "probe exited", process.poll())
      results.append(json.loads(line))
    expected, actual = results
    logs = expected.pop("logs")
    native_logs = []
    for expected_log in logs:
      packet = self.socket.recv()
      assert packet[0] == expected_log["levelnum"]
      record = json.loads(packet[1:])
      native_log = {key: record[key] for key in expected_log}
      assert native_log == expected_log, (native_log, expected_log)
      native_logs.append(native_log)
    passed = normal(actual) == normal(expected)
    self.evidence.write(json.dumps({"scenario": self.label, "request": request, "expected": expected, "actual": actual, "logs": native_logs, "passed": passed}) + "\n")
    self.evidence.flush()
    emulation_limit = bool(os.environ.get("VERSION_EMULATED_SPAWN")) and actual == {"value": ""} and (
      (self.label == "boundaries" and op == "git" and fields.get("method") == "commit" and fields.get("cwd", "").endswith("/not-yet-present")) or
      (self.label == "missing-executable" and op == "git" and fields.get("method") == "origin"))
    assert passed or emulation_limit, (self.label, request, expected, actual)
    self.count += 1
    return actual

  def close(self):
    for process in self.processes:
      assert process.stdin is not None
      process.stdin.close()
      assert process.wait(timeout=10) == 0
    assert not self.socket.poll(100), (self.label, "unexpected native logs")
    self.socket.close()
    self.context.term()
    self.endpoint_path.unlink(missing_ok=True)
    for error in self.errors:
      error.close()


@contextmanager
def pair(binary, cwd, directory, label, environment, evidence):
  probes = Pair(binary, cwd, directory, label, environment, evidence)
  try:
    yield probes
  finally:
    probes.close()


def main() -> None:
  binary = Path(sys.argv[1]).resolve()
  invocation = [*sys.argv[2:], str(binary)]
  output = Path(os.environ.get("VERSION_EVIDENCE", ".omo/evidence/runtime-version-73/reference")).resolve()
  output.mkdir(parents=True, exist_ok=True)
  # Disable machine/global Git policy in child fixtures, never modify actual configuration.
  environment = {**os.environ, "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1", "GIT_AUTHOR_DATE": "2020-01-02T03:04:05+00:00", "GIT_COMMITTER_DATE": "2020-01-02T03:04:05+00:00"}
  os.environ.update({key: environment[key] for key in environment if key.startswith("GIT_")})
  counts = {}
  with tempfile.TemporaryDirectory(prefix="rust-version-") as temporary, (output / "comparisons.jsonl").open("w") as evidence:
    root = Path(temporary)
    for name, tracking, origin in [("clean", True, True), ("dirty", True, True), ("staged", True, True), ("untracked", True, True), ("prebuilt", False, True), ("nontracking", False, True), ("missingorigin", False, False), ("detached", True, True), ("trackingremote", True, True)]:
      repo = fixture(root / name, tracking, origin)
      match name:
        case "dirty" | "staged":
          (repo / "tracked").write_text("modified\n")
          if name == "staged": git(repo, "add", "tracked")
        case "untracked": (repo / "untracked").write_text("ignored by diff")
        case "prebuilt": (repo / "prebuilt").touch()
        case "detached": git(repo, "checkout", "--detach", "HEAD")
        case "trackingremote":
          git(repo, "remote", "add", "upstream", "https://example.invalid/fork.git")
          git(repo, "update-ref", "refs/remotes/upstream/main", "HEAD")
          git(repo, "branch", "--set-upstream-to=upstream/main")
      with pair(invocation, repo, output, name, environment, evidence) as probes:
        probes.call("metadata", path=str(repo))
        for method in ["commit", "commit_date", "short_branch", "branch", "origin", "normalized_origin"]:
          probes.call("git", method=method, cwd=str(repo))
        probes.call("dirty", path=str(repo))
        probes.call("prebuilt", path=str(repo))
        counts[name] = probes.count

    repo = fixture(root / "cwd-oddity")
    target = root / "prebuilt-without-git"
    target.mkdir()
    (target / "prebuilt").touch()
    with pair(invocation, repo, output, "dirty-process-cwd", environment, evidence) as probes:
      probes.call("dirty", path=str(target))
      probes.call("chdir", path=str(target))
      probes.call("dirty", path=str(repo))
      counts["dirty-process-cwd"] = probes.count
    with pair(invocation, target, output, "dirty-cwd-missing-origin", environment, evidence) as probes:
      (repo / "prebuilt").touch()
      probes.call("dirty", path=str(repo))
      counts["dirty-cwd-missing-origin"] = probes.count

    repo = fixture(root / "cache")
    elsewhere = fixture(root / "elsewhere", tracking=False, origin=False)
    with pair(invocation, repo, output, "cache-and-cwd", environment, evidence) as probes:
      for op in ["metadata", "dirty", "prebuilt"]: probes.call(op, path=str(repo))
      probes.call("git", method="commit", cwd=str(repo), revision="missing")
      (repo / "tracked").write_text("changed\n")
      (repo / "prebuilt").touch()
      git(repo, "add", ".")
      git(repo, "commit", "-m", "second")
      git(repo, "branch", "missing")
      git(repo, "remote", "set-url", "origin", "https://changed.invalid/repo.git")
      for op in ["metadata", "dirty", "prebuilt"]: probes.call(op, path=str(repo))
      probes.call("git", method="commit", cwd=str(repo), revision="missing")
      probes.call("git", method="commit", cwd=str(repo))
      probes.call("git", method="commit_date", cwd=str(repo))
      probes.call("chdir", path=str(elsewhere))
      probes.call("dirty", path=str(elsewhere))
      probes.call("git", method="origin")
      probes.call("git", method="normalized_origin")
      counts["cache-and-cwd"] = probes.count

    repo = fixture(root / "empty-cache", tracking=False, origin=False)
    with pair(invocation, repo, output, "empty-default-cache", environment, evidence) as probes:
      probes.call("git", method="origin", cwd=str(repo))
      git(repo, "remote", "add", "origin", "https://added.invalid/repo")
      probes.call("git", method="origin", cwd=str(repo))
      probes.call("git", method="origin", cwd=str(repo) + "/.")
      counts["empty-default-cache"] = probes.count

    repo = fixture(root / "dirty-exception")
    config = repo / ".git/config"
    config.write_bytes(config.read_bytes().replace(b"git@github.com:commaai/openpilot.git", b"https://bad\xff.invalid/repo"))
    with pair(invocation, repo, output, "dirty-exception-cache", environment, evidence) as probes:
      probes.call("dirty", path=str(repo))
      git(repo, "remote", "set-url", "origin", "https://fixed.invalid/repo")
      probes.call("dirty", path=str(repo))
      counts["dirty-exception-cache"] = probes.count

    repo = fixture(root / "boundaries")
    empty = root / "empty"
    empty.mkdir()
    with pair(invocation, repo, output, "boundaries", environment, evidence) as probes:
      probes.call("metadata", path=str(empty))
      probes.call("dirty", path=str(empty))
      probes.call("git", method="origin", cwd=str(empty))
      missing = root / "not-yet-present"
      probes.call("git", method="commit", cwd=str(missing))
      fixture(missing)
      probes.call("git", method="commit", cwd=str(missing))
      config = repo / ".git/config"
      config.write_bytes(config.read_bytes().replace(b"git@github.com:commaai/openpilot.git", b"https://bad\xff.invalid/repo"))
      probes.call("git", method="origin", cwd=str(repo))
      probes.call("dirty", path=str(repo))
      git(repo, "remote", "set-url", "origin", "https://recovered.invalid/repo")
      probes.call("git", method="origin", cwd=str(repo))
      probes.call("dirty", path=str(repo))
      (repo / "RELEASES.md").write_bytes(b"one\r\ntwo\r\n\r\nsecond")
      probes.call("release_notes", path=str(repo))
      (repo / "openpilot/common/version.h").write_bytes(b'bad\xff"1"')
      probes.call("version", path=str(repo))
      (repo / "openpilot/common/version.h").write_text("no quoted version")
      probes.call("version", path=str(repo))
      (repo / "openpilot/common/version.h").write_text('before "a" after "b"')
      probes.call("version", path=str(repo))
      # Existing .git files count too, as in Git worktrees.
      worktree = root / "gitfile"
      git(repo, "worktree", "add", "--detach", str(worktree), "HEAD")
      probes.call("metadata", path=str(worktree))
      unreadable = root / "unreadable"
      unreadable.mkdir()
      unreadable.chmod(0)
      try:
        probes.call("metadata", path=str(unreadable))
      finally:
        unreadable.chmod(0o700)
      (empty / "build.json").write_text('{}')
      probes.call("metadata", path=str(empty))
      (empty / "build.json").unlink()
      (empty / "build.json").symlink_to("build.json")
      probes.call("metadata", path=str(empty))
      counts["boundaries"] = probes.count

    repo = fixture(root / "json")
    with pair(invocation, repo, output, "build-json", environment, evidence) as probes:
      (repo / "build.json").write_text('{}')
      probes.call("metadata", path=str(repo))
      (repo / "build.json").write_text('{"channel":"nightly","openpilot":{"version":"fresh"}}')
      probes.call("metadata", path=str(repo))
      (repo / "build.json").write_text('{')
      probes.call("metadata", path=str(repo))
      (repo / "build.json").write_bytes(b'\xff')
      probes.call("metadata", path=str(repo))
      (repo / "build.json").unlink()
      (repo / "build.json").mkdir()
      probes.call("metadata", path=str(repo))
      cases = ['{}', '[]', 'null', '{"openpilot":null}', '{"channel":null}', '{"channel":42}', '{"channel":["nightly"]}', '{"openpilot":{"is_dirty":true}}']
      cases += [json.dumps({"channel": channel}) for channel in ["release-tizi-staging", "release-mici-staging", "release-tizi", "release-mici", "nightly", "devel-staging", "nightly-dev", "nightly-dev-x", "unknown"]]
      cases += [json.dumps({"openpilot": {"git_origin": origin}}) for origin in ["git@github.com:commaai/openpilot.git", "https://github.com/commaai/openpilot.git", "git@github.com:fork/openpilot.git", "xgit@git@host.git.githttps://https://:tail", "http://github.com/commaai/openpilot", "ssh://git@github.com/commaai/openpilot.git", "github.com/commaai/openpilot/", "\ud800:abc.git"]]
      cases += [json.dumps({"channel": value, "openpilot": {"version": value, "git_commit": value, "git_origin": value, "release_notes": value, "git_commit_date": value, "build_style": value}}) for value in [None, True, False, 42, -(10**100), 1.234, -0.0, float("nan"), float("inf"), "a-b-c", "한글😀abcd", "\ud800abc", [], ["a", "\ud800", "\x1c", "\u200b", "\ue000", "\u0378", "'\""], {"z": 1, "a": "value"}]]
      cases += ['{"openpilot":{"version":"old","version":"new"}}', '{"openpilot":{"version":1e999}}', '{"openpilot":{"version":' + '1' * 4301 + '}}']
      for source in cases: probes.call("from_json", source=source)
      counts["build-json"] = probes.count

    repo = fixture(root / "missing-executable")
    bindir = root / "bin"
    bindir.mkdir()
    with pair(invocation, repo, output, "missing-executable", {**environment, "PATH": str(bindir)}, evidence) as probes:
      probes.call("git", method="origin", cwd=str(repo))
      (bindir / "git").symlink_to(subprocess.check_output(["which", "git"], text=True).strip())
      probes.call("git", method="origin", cwd=str(repo))
      counts["missing-executable"] = probes.count
  comparisons = [json.loads(line) for line in (output / "comparisons.jsonl").read_text().splitlines()]
  mismatches = [entry for entry in comparisons if not entry["passed"]]
  manifest = {"matching_comparisons": len(comparisons) - len(mismatches), "divergences": mismatches, "counts": counts, "comparisons": sum(counts.values()), "binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "invocation": invocation, "python": sys.version, "source_sha256": {source: hashlib.sha256((ROOT / source).read_bytes()).hexdigest() for source in SOURCES}, "result": "EMULATION_LIMITS" if os.environ.get("VERSION_EMULATED_SPAWN") else "PASS"}
  (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
  print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
  if sys.argv[1:] == ["--source"]:
    source_worker()
  else:
    main()
