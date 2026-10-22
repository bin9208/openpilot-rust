#!/usr/bin/env python3
"""Isolated, unpublished full-build comparison; never publish or access a device."""
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import time

ROOT = Path(os.environ["SOURCE_ROOT"])
OUTPUT = Path(os.environ["BENCH_ROOT"]) / "comparison"
OUTPUT.mkdir(exist_ok=True)


def resources():
    result = {"disk_free": shutil.disk_usage(OUTPUT).free}
    for name, path in (("memory", "/proc/meminfo"), ("load", "/proc/loadavg"),
                       ("memory_events", "/sys/fs/cgroup/memory.events"),
                       ("memory_peak", "/sys/fs/cgroup/memory.peak")):
        try:
            result[name] = Path(path).read_text()
        except OSError:
            pass
    return result


def bind_source():
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    assert sha == os.environ["GITHUB_SHA"] == os.environ["RUNTIME_SOURCE_SHA"]
    assert os.environ["GITHUB_ACTIONS"] == "true"
    assert os.environ["OPENPILOT_RUNNER_COMPARISON"] == "498"
    assert os.environ["GITHUB_REPOSITORY"] == "bin9208/openpilot-rust"
    return sha


def stage(label, script):
    sha = bind_source()
    before = resources()
    begin = time.monotonic()
    env = dict(os.environ, GITHUB_WORKSPACE=str(ROOT))
    with (OUTPUT / (label + ".log")).open("w") as log:
        process = subprocess.Popen(["/usr/bin/time", "-v", "bash", "-euo", "pipefail", script],
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=env)
        for line in iter(process.stdout.readline, b""):
            decoded = line.decode(errors="replace")
            log.write(decoded)
            log.flush()
            print(decoded, end="", flush=True)
        code = process.wait()
    row = dict(stage=label, seconds=round(time.monotonic() - begin, 3), exit_code=code,
               source_commit=sha, runner=os.environ["RUNNER_NAME"],
               runner_environment=os.environ["RUNNER_ENVIRONMENT"], before=before, after=resources())
    with (OUTPUT / "timing.jsonl").open("a") as stream:
        stream.write(json.dumps(row) + "\n")
    print("MEASURED", json.dumps({key: row[key] for key in ("stage", "seconds", "exit_code")}), flush=True)
    raise SystemExit(code)


def package(command):
    bind_source()
    assert command in ("source-policy", "prepare", "host-tools", "build")
    os.environ["GITHUB_WORKSPACE"] = str(ROOT)
    delivery = runpy.run_path(str(ROOT / "rust/ci/runtime/package.py"))
    if command == "source-policy":
        delivery["check_source_policy"]()
    elif command == "prepare":
        delivery["stage_baseline"]()
        delivery["sysroot"]()
        delivery["environment"]()
    elif command == "host-tools":
        delivery["host_tools"]()
    else:
        delivery["build"]()


if __name__ == "__main__":
    if sys.argv[1] == "stage":
        stage(sys.argv[2], sys.argv[3])
    elif sys.argv[1] == "package":
        package(sys.argv[2])
    else:
        raise SystemExit("stage LABEL SHELL_FILE | package source-policy/prepare/host-tools/build")
