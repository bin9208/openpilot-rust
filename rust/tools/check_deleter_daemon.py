import argparse
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import tempfile
import time
import types

from check_deleter_reference import SOURCE, Source, load_definitions, snapshot


def run(binary, root, extra=(), stop=None):
  environment = dict(os.environ, LOG_ROOT=str(root))
  with subprocess.Popen([str(binary), *extra], env=environment, stderr=subprocess.PIPE, text=True) as process:
    lines, times = [], []
    with selectors.DefaultSelector() as selector:
      selector.register(process.stderr, selectors.EVENT_READ)
      deadline = time.monotonic() + 5
      ready = False
      while process.poll() is None:
        assert time.monotonic() < deadline, "daemon timeout"
        if selector.select(.1):
          line = process.stderr.readline()
          lines.append(line)
          if "deleting " in line:
            times.append(time.monotonic())
          if "deleter: ready" in line:
            ready = True
            if stop:
              time.sleep(.05)
              before = time.monotonic()
              process.send_signal(stop)
      lines.extend(process.stderr.readlines())
    result = {"exit": process.wait(), "stderr": "".join(lines), "ready": ready,
              "deletion_intervals": [right - left for left, right in zip(times, times[1:], strict=False)]}
    if stop:
      result["shutdown_seconds"] = time.monotonic() - before
    return result


def low_space(binary, output):
  mount = output / "private-tmpfs"
  mount.mkdir()
  subprocess.run(["mount", "-t", "tmpfs", "-o", "size=16m", "tmpfs", str(mount)], check=True)
  try:
    original, rust = mount / "original", mount / "rust"
    for root in (original, rust):
      root.mkdir()
      for name in ("route--0", "route--1", "route--2", "route--3", "boot"):
        (root / name).mkdir()
        (root / name / "rlog.zst").write_bytes(b"synthetic")
      (root / "route--0" / "inuse.lock").touch()
    stats = os.statvfs(mount)
    assert stats.f_bavail * stats.f_frsize < 5 * 1024**3
    source = Source(original)
    load_definitions(SOURCE / "config.py", source.namespace, {"get_available_bytes", "get_available_percent"})
    waits = []

    def wait(seconds):
      waits.append(seconds)
      if len(waits) < 3:
        time.sleep(seconds)

    event = types.SimpleNamespace(is_set=lambda: len(waits) == 3, wait=wait)
    source.namespace["deleter_thread"](event)
    result = run(binary, rust, ("--cycles", "3"))
    assert result["exit"] == 0 and result["ready"], result
    assert snapshot(original) == snapshot(rust), (snapshot(original), snapshot(rust))
    assert waits == [.1, .1, .1]
    assert len(result["deletion_intervals"]) == 2, result
    assert all(.09 <= interval < .5 for interval in result["deletion_intervals"]), result
    result.update(available_bytes=stats.f_bavail * stats.f_frsize, source_waits=waits, remaining=snapshot(rust))
    return result
  finally:
    subprocess.run(["umount", str(mount)], check=True)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--private-mount", action="store_true")
  parser.add_argument("--privileged-mount", action="store_true")
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  binary = args.binary.resolve()
  if args.private_mount:
    result = low_space(binary, args.output)
    (args.output / "low-space.json").write_text(json.dumps(result, indent=2))
    return
  rows = []
  with tempfile.TemporaryDirectory(prefix="deleter-cli-", dir=args.output) as directory:
    absent = Path(directory) / "absent"
    for sig in (signal.SIGINT, signal.SIGTERM):
      result = run(binary, absent, stop=sig)
      assert result["exit"] == 0 and result["shutdown_seconds"] < .3, result
      assert not absent.exists()
      rows.append(result)
    for flags in (("--cycles", "0"), ("--cycles", "-1"), ("--unknown",)):
      result = run(binary, absent, flags)
      assert result["exit"] == 1 and not absent.exists(), result
      rows.append(result)
    result = run(binary, absent, ("--cycles", "1"))
    assert result["exit"] == 0 and not absent.exists(), result
    rows.append(result)
  namespace = ["sudo", "-n", "unshare", "--mount"] if args.privileged_mount else ["unshare", "--user", "--map-root-user", "--mount"]
  interpreter = "/usr/bin/python3" if args.privileged_mount else sys.executable
  child = subprocess.run([*namespace, interpreter, __file__,
                          "--binary", str(binary), "--output", str(args.output.resolve()), "--private-mount"],
                         text=True, capture_output=True, timeout=20)
  (args.output / "namespace.log").write_text(child.stdout + child.stderr)
  assert child.returncode == 0, child.stderr
  low = json.loads((args.output / "low-space.json").read_text())
  report = {"result": "pass", "idle_and_cli": rows, "real_low_space": low,
            "scope": "private 16 MiB tmpfs in a mount namespace; synthetic files only"}
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
