#!/usr/bin/env python3
"""Host-only native IPC, deadline and signal checks for the continuous procLog daemon."""
from __future__ import annotations

import argparse
import ast
from contextlib import contextmanager
import errno
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
from types import ModuleType, SimpleNamespace

import capnp
import msgq

ROOT = Path(__file__).resolve().parents[2]
MEMORY = b"MemTotal: 128 kB\nMemFree: 16 kB\n"


def source_cadence(binary: Path, output: Path) -> None:
    """Run the actual Ratekeeper class with a controlled monotonic clock."""
    module = ModuleType("ratekeeper_reference")
    for relative, name in [("openpilot/common/utils.py", "MovingAverage"),
                           ("openpilot/common/realtime.py", "Ratekeeper")]:
        path = ROOT / relative
        tree = ast.parse(path.read_text(), filename=str(path))
        tree.body = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == name]
        exec(compile(tree, str(path), "exec"), module.__dict__)
    now = 0.0
    module.time = SimpleNamespace(monotonic=lambda: now)
    module.getproctitle = lambda: "proclog-reference"
    reference = module.Ratekeeper(0.5, print_delay_threshold=None)
    # Irregular work, exact deadlines, large overruns and eventual catch-up.
    times = [700_000_000, 2_900_000_000, 10_000_000_000, 10_000_000_000, 10_000_000_000]
    times += [10_000_000_000 + i * 1_700_000_000 for i in range(100)]
    expected = []
    for value in times:
        now = value / 1e9
        reference.monitor_time()
        expected.append(round(max(0, reference.remaining) * 1e9))
    result = subprocess.run([binary], input="".join(f"{value}\n" for value in times),
                            text=True, capture_output=True, check=True)
    observed = [int(line) for line in result.stdout.splitlines()]
    assert len(observed) == len(expected)
    assert all(abs(a - b) <= 1 for a, b in zip(observed, expected, strict=True)), (observed, expected)
    (output / "source-cadence.json").write_text(json.dumps({"now_ns": times, "source_wait_ns": expected,
                                                          "rust_wait_ns": observed}, indent=2) + "\n")


@contextmanager
def daemon(binary: Path, destination: Path, args: list[str]):
    """Own a disposable original-runtime namespace and clean up on failures."""
    with tempfile.TemporaryDirectory(prefix="msgq_proclog-runtime-", dir="/dev/shm") as directory:
        prefix = Path(directory).name.removeprefix("msgq_")
        os.environ["OPENPILOT_PREFIX"] = prefix
        os.environ.pop("CEREAL_FAKE", None)
        os.environ.pop("ZMQ", None)
        subscriber = msgq.sub_sock("procLog", timeout=100, segment_size=10 * 1024 * 1024)
        assert subscriber.receive() is None, "native subscriber must time out before publication"
        with (destination / "daemon.log").open("w") as log:
            process = subprocess.Popen([binary, *args], stdout=log, stderr=log)
            try:
                yield process, subscriber
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=5)
                del subscriber


def receive(subscriber, destination: Path) -> dict:
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        packet = subscriber.receive()
        if packet is not None:
            destination.write_bytes(packet)
            with SCHEMA.Event.from_bytes(packet) as message:
                assert message.which() == "procLog" and message.valid
                return message.to_dict()
    raise TimeoutError(f"no native procLog message: {destination}")


class CollectionGate:
    """A FIFO in synthetic procfs holds the real executable inside collection."""
    def __init__(self, directory: Path):
        self.root = directory / "proc"
        self.root.mkdir()
        (self.root / "stat").write_text("cpu 0 0 0 0 0 0 0\ncpu0 100 2 30 400 5 6 7\n")
        self.path = self.root / "meminfo"
        os.mkfifo(self.path)

    @contextmanager
    def hold(self):
        deadline = time.monotonic() + 5
        while True:
            try:
                descriptor = os.open(self.path, os.O_WRONLY | os.O_NONBLOCK)
                break
            except OSError as error:
                if error.errno != errno.ENXIO or time.monotonic() >= deadline:
                    raise
                time.sleep(0.005)
        try:
            yield
        finally:
            os.write(descriptor, MEMORY)
            os.close(descriptor)


def blocked_cadence(binary: Path, output: Path) -> dict:
    destination = output / "blocked-cadence"
    destination.mkdir()
    gate = CollectionGate(destination)
    messages, released = [], []
    started = time.monotonic_ns()
    with daemon(binary, destination, ["--proc-root", str(gate.root), "--frames", "4"]) as (process, subscriber):
        for index, delay in enumerate([0.25, 4.4, 0, 0]):
            with gate.hold():
                subscriber.receive(non_blocking=True)  # reconnect after native publisher initialization
                time.sleep(delay)
                released.append(time.monotonic_ns())
            messages.append(receive(subscriber, destination / f"message-{index}.capnp"))
        assert process.wait(timeout=1) == 0
        assert subscriber.receive() is None, "bounded daemon emitted extra messages"
    timestamps = [message["logMonoTime"] for message in messages]
    assert all(started <= stamp < release for stamp, release in zip(timestamps, released, strict=True))
    assert released[0] - timestamps[0] >= 250_000_000
    assert 1.8e9 <= timestamps[1] - released[0] <= 2.5e9, "first deadline must follow first publication"
    assert released[1] - timestamps[1] >= 4.4e9
    assert 0 < timestamps[3] - timestamps[2] < 500_000_000, "overrun must catch up without deadline reset"
    for message in messages:
        assert message["procLog"]["mem"]["total"] == 128 * 1024
        assert len(message["procLog"]["cpuTimes"]) == 1
    assert "lagging by" in (destination / "daemon.log").read_text()
    return {"result": "pass", "timestamps_ns": timestamps, "collection_release_ns": released, "exit": 0}


def signals(binary: Path, output: Path) -> list[dict]:
    results = []
    for signum in (signal.SIGINT, signal.SIGTERM):
        for collecting in (False, True):
            destination = output / f"{signum.name}-{'collecting' if collecting else 'sleeping'}"
            destination.mkdir()
            gate = CollectionGate(destination) if collecting else None
            args = ["--proc-root", str(gate.root)] if gate else []
            with daemon(binary, destination, args) as (process, subscriber):
                interval = None
                if gate:
                    with gate.hold():
                        subscriber.receive(non_blocking=True)
                        sent = time.monotonic()
                        process.send_signal(signum)
                        time.sleep(0.1)
                    assert process.wait(timeout=1) == 0
                    elapsed = time.monotonic() - sent
                    assert subscriber.receive() is None, "shutdown during collection must not publish"
                else:
                    first = receive(subscriber, destination / "first.capnp")
                    second = receive(subscriber, destination / "second.capnp")
                    interval = (second["logMonoTime"] - first["logMonoTime"]) / 1e9
                    assert 1.5 < interval < 2.5, interval
                    assert any(proc["pid"] == process.pid for proc in second["procLog"]["procs"])
                    assert second["procLog"]["mem"]["total"] > 0 and second["procLog"]["cpuTimes"]
                    sent = time.monotonic()
                    process.send_signal(signum)
                    assert process.wait(timeout=1) == 0
                    elapsed = time.monotonic() - sent
                assert elapsed < 1
                results.append({"signal": signum.name, "collecting": collecting, "exit": 0,
                                "signal_to_exit_seconds": elapsed, "live_interval_seconds": interval})
    return results


def cli(binary: Path, output: Path) -> None:
    checks = []
    for args in (["--help"], ["--frames", "0"], ["--frames"], ["--frames", "-1"],
                 ["--frames", "1", "--frames", "2"], ["--interval-ms", "1"],
                 ["--proc-root"], ["--unknown"]):
        result = subprocess.run([binary, *args], capture_output=True, text=True, timeout=3)
        assert result.returncode == (0 if args == ["--help"] else 1)
        checks.append({"args": args, "exit": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
    (output / "cli.json").write_text(json.dumps(checks, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--cadence-binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    SCHEMA = capnp.load(str(ROOT / "openpilot/cereal/log.capnp"),
                        imports=[str(ROOT / "openpilot/cereal"), str(ROOT / "opendbc_repo/opendbc/car")])
    source_cadence(args.cadence_binary.resolve(), args.output)
    cli(args.binary.resolve(), args.output)
    report = {"scope": "host native IPC and source-derived timing; no vehicle or performance claim",
              "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              "blocked_cadence": blocked_cadence(args.binary.resolve(), args.output),
              "signals": signals(args.binary.resolve(), args.output), "result": "pass"}
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
