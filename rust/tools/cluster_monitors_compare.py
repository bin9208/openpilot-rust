from __future__ import annotations

import argparse
from contextlib import redirect_stdout
from dataclasses import asdict
import hashlib
import io
import json
from pathlib import Path
import random
import subprocess
import sys
from types import SimpleNamespace


def compare(expected, actual, where="root"):
  if isinstance(expected, dict):
    assert expected.keys() == actual.keys(), (where, expected.keys(), actual.keys())
    for key in expected:
      compare(expected[key], actual[key], f"{where}.{key}")
  elif isinstance(expected, list):
    assert len(expected) == len(actual), (where, len(expected), len(actual))
    for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
      compare(left, right, f"{where}[{index}]")
  else:
    assert expected == actual, (where, expected, actual)


def make_stat(root, pid, tid, ticks=10, start=123, core=6, malformed=False):
  path = root / str(pid) / "task" / str(tid) / "stat"
  path.parent.mkdir(parents=True, exist_ok=True)
  fields = ["0"] * 37
  fields[0], fields[11], fields[12], fields[19], fields[36] = "S", str(ticks), "3", str(start), str(core)
  path.write_text("invalid" if malformed else f"{tid} (cluster (worker)) " + " ".join(fields))


def source_proc_cases(output, source):
  root = output / "proc"
  for pid, name, argv in [
    (42, "python", b"python\0/carrot/cluster/main.py\0"),
    (50, "ffmpeg", b"/bin/ffmpeg\0"),
    (51, "cluster-encode", b"/bin/cluster-encode\0"),
    (52, "other", b"/bin/other\0"),
  ]:
    make_stat(root, pid, pid)
    (root / str(pid) / "comm").write_text(name)
    (root / str(pid) / "cmdline").write_bytes(argv)
  (root / "42/task/42/children").write_text("50 51 invalid 52 -1")
  make_stat(root, 42, 43, ticks=8, core=7)
  make_stat(root, 42, 44, malformed=True)
  make_stat(root, 42, -2, ticks=500)
  make_stat(root, 51, 55, ticks=5, core=-1)
  sampler = source.ClusterProcessCoreUsageSampler()
  source.os = SimpleNamespace(getpid=lambda: 42)
  results = []
  roots = [root, output / "absent-proc"]
  for path in roots:
    source.PROC_PATH = path
    samples, candidates, matched = sampler._read_cluster_thread_samples()
    results.append({"samples": [asdict(sample) for sample in samples], "candidate_processes": candidates, "matched_processes": matched})
  sampler.close()
  return roots, results


def source_core_cases(source):
  rng = random.Random(253)
  requests, expected = [], []
  for clock_ticks in [100, 250, 0, -1]:
    sampler = source.ClusterProcessCoreUsageSampler(debug=True)
    sampler._clock_ticks = clock_ticks
    original_format = sampler._format_percent_map
    maps = []

    def record(values, maps=maps, original_format=original_format):
      maps.append(list(values.items()))
      return original_format(values)

    sampler._format_percent_map = record
    updates, results = [], []
    now = 0.0
    ticks = [100, 200, 300]
    for index in range(80):
      now += [0.0, 0.001, 0.1, 0.5, 1.0][index % 5]
      samples = []
      for thread in range(3):
        ticks[thread] += rng.choice([-2, 0, 1, 2, 8, 50])
        sample = source._ThreadCpuSample(
          (42, 43 + thread, index // 17), 42, "cluster" if thread != 1 else "cluster-encode", ticks[thread], rng.choice([-1, 0, 4, 6, 7])
        )
        samples.append(sample)
      if index % 19 == 0:
        samples = []
      updates.append({"now": now, "samples": [asdict(sample) for sample in samples]})
      source.time = SimpleNamespace(perf_counter=lambda value=now: value)
      sampler._read_cluster_thread_samples = lambda value=samples: (value, 3, 2)
      maps.clear()
      captured = io.StringIO()
      with redirect_stdout(captured):
        sampler._refresh()
      active = int(captured.getvalue().split("active_threads=")[1].split()[0])
      results.append({"text": sampler._text, "by_core": maps[0], "by_process": maps[1], "active_threads": active})
    sampler.close()
    requests.append({"clock_ticks": clock_ticks, "updates": updates})
    expected.append(results)
  return requests, expected


def source_git_cases(output, source):
  repo = output / "repo"
  (repo / ".git").mkdir(parents=True)
  (repo / ".git/HEAD").write_text("ref: refs/heads/dev\n")
  worktree = output / "worktree"
  worktree.mkdir()
  (worktree / ".git").write_text("GITDIR: ../repo/.git\n")
  detached = output / "detached"
  (detached / ".git").mkdir(parents=True)
  (detached / ".git/HEAD").write_text("0123456789abcdef0123456789abcdef01234567")
  missing = output / "missing"
  roots = [repo, worktree, detached, missing]
  repositories = [{"head": source.read_head_branch(path), "git_dir": source.resolve_git_dir(path)} for path in roots]
  plans = [
    (None, []),
    (detached, [(0, "")]),
    (repo, [(124, "")]),
    (repo, [(0, "dev"), (0, "")]),
    (repo, [(0, "dev"), (1, "origin")]),
  ]
  for remote in ["origin\n", "a\na/b\n", "upstream\nother\n"]:
    for upstream in ["", "a/b/topic", "unknown/topic"]:
      for remote_code, fetch_code, count in [(2, None, None), (124, None, None), (0, 1, None), (0, 0, "invalid"), (0, 0, "-3"), (0, 0, "0"), (0, 0, "12")]:
        outputs = [(0, "dev\n"), (0, remote), (0, upstream), (remote_code, "")]
        if fetch_code is not None:
          outputs.append((fetch_code, ""))
        if count is not None:
          outputs.append((0, count))
        plans.append((repo, outputs))
  requests, expected = [], []
  for root, outputs in plans:
    provider = source.GitBranchStatusProvider(root or missing, remote_enabled=False)
    provider.repo_path = root
    pending, calls = list(outputs), []

    def git(*args, timeout_s, calls=calls, pending=pending):
      calls.append({"args": args, "timeout": timeout_s})
      code, stdout = pending.pop(0)
      return subprocess.CompletedProcess(args, code, stdout, "")

    provider._git = git
    expected.append({"status": asdict(provider._read_status()), "calls": calls})
    assert not pending
    requests.append({"repo": root, "outputs": [{"code": code, "stdout": stdout} for code, stdout in outputs]})
  return roots, repositories, requests, expected


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("root", type=Path)
  parser.add_argument("binary", type=Path)
  parser.add_argument("output", type=Path)
  args = parser.parse_args()
  root, binary, output = (value.resolve() for value in (args.root, args.binary, args.output))
  output.mkdir(parents=True, exist_ok=False)
  sys.path.insert(0, str(root))
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_system_monitor as monitors
  import cluster_git_status as git

  addresses = [
    b"",
    b"0.0.0.0",
    b"127.1.2.3",
    b"169.254.8.9",
    b"::",
    b"::1",
    b"fe80::123%eth0",
    b"192.168.001.2",
    b"host",
    b" 192.0.2.1 ",
    b"2001:0DB8::AB%eth0",
    b"::ffff:192.0.2.1",
    b"2001::1%a%b",
    b"2001::1%",
    b"255.255.255.255",
    b"ff02::1",
    b"::ffff:127.0.0.1",
    b"::ffff:0.0.0.0",
    b"::ffff:169.254.1.1",
    b"\xff",
    b"192.0.2.1\xff",
    b"2001::1%\xff",
  ]
  sequences, histories = source_core_cases(monitors)
  proc_roots, scans = source_proc_cases(output, monitors)
  repos, repository_results, git_cases, git_results = source_git_cases(output, git)
  request = {"addresses": [list(value) for value in addresses], "sequences": sequences, "proc_roots": proc_roots, "repos": repos, "git": git_cases}
  expected = {
    "addresses": [monitors.NetworkAddressProvider._valid_address(value) for value in addresses],
    "histories": histories,
    "scans": scans,
    "repos": repository_results,
    "git": git_results,
  }

  def encode(value):
    return json.dumps(value, default=str, ensure_ascii=False)

  request_text = encode(request)
  expected = json.loads(encode(expected))
  (output / "input.json").write_text(request_text)
  (output / "source.json").write_text(encode(expected))
  provider_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
  run = subprocess.run([str(binary)], input=request_text, capture_output=True, text=True, check=True)
  actual = json.loads(run.stdout)
  (output / "native.json").write_text(encode(actual))
  compare(expected, actual)
  result = {
    "python_version": sys.version,
    "addresses": len(addresses),
    "ordered_core_updates": sum(len(sequence["updates"]) for sequence in sequences),
    "proc_scans": len(scans),
    "git_repositories": len(repos),
    "git_status_cases": len(git_results),
    "binary_sha256": provider_hash,
    "source_sha256": {
      name: hashlib.sha256((root / "openpilot/selfdrive/carrot/cluster" / name).read_bytes()).hexdigest()
      for name in ["cluster_system_monitor.py", "cluster_git_status.py"]
    },
    "result": "PASS",
  }
  (output / "result.json").write_text(json.dumps(result, indent=2))
  print(json.dumps(result))


if __name__ == "__main__":
  main()
