from __future__ import annotations

import argparse
from contextlib import contextmanager
from hashlib import sha256
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
from collections.abc import Callable, Iterator

import msgq
import zmq
from openpilot.cereal.services import SERVICE_LIST

NAMES = ("logMessage", "carState")


def port(name: str) -> int:
    value = 0xcbf29ce484222325
    for byte in name.encode():
        value = ((value ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    return 8023 + value % (65535 - 8023)


def wait(predicate: Callable[[], bool]) -> None:
    deadline = time.monotonic() + 8
    while not predicate():
        assert time.monotonic() < deadline, "bridge readiness deadline"
        time.sleep(.01)


@contextmanager
def running(binary: Path, args: list[str], output: Path) -> Iterator[subprocess.Popen[bytes]]:
    with (output / "stdout").open("wb") as stdout, (output / "stderr").open("wb") as stderr:
        with subprocess.Popen([str(binary), *args], stdout=stdout, stderr=stderr) as process:
            try:
                yield process
            finally:
                if process.poll() is None:
                    process.send_signal(signal.SIGTERM)
                try:
                    status = process.wait(4)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                    raise
                assert status == 0, (binary, status, (output / "stderr").read_text())


def mapped(process: subprocess.Popen[bytes], name: str) -> bool:
    return f"/{name}\n" in Path(f"/proc/{process.pid}/maps").read_text()


def outgoing(binary: Path, output: Path) -> str:
    with tempfile.TemporaryDirectory(prefix="msgq_rust-probe-bridge-", dir="/dev/shm") as namespace:
        os.environ["OPENPILOT_PREFIX"] = Path(namespace).name.removeprefix("msgq_")
        context = msgq.Context()
        with zmq.Context() as wire, running(binary, [], output) as process:
            time.sleep(.25)
            assert process.poll() is None and not list(Path(namespace).iterdir())
            publishers = {}
            for name in NAMES:
                publisher = msgq.PubSocket()
                publisher.connect(context, name, SERVICE_LIST[name].queue_size)
                publishers[name] = publisher
            with wire.socket(zmq.SUB) as first, wire.socket(zmq.SUB) as second:
                for socket, name in zip((first, second), NAMES, strict=True):
                    socket.setsockopt(zmq.LINGER, 0)
                    socket.setsockopt(zmq.RCVTIMEO, 5000)
                    socket.setsockopt(zmq.SUBSCRIBE, b"")
                    socket.connect(f"tcp://127.0.0.1:{port(name)}")
                wait(lambda: all(mapped(process, name) for name in NAMES))
                time.sleep(.15)
                packets = [number.to_bytes(4, "little") + b"\x00\xffbridge" for number in range(160)]
                for packet in packets:
                    for publisher in publishers.values():
                        publisher.send(packet)
                for socket in (first, second):
                    assert [socket.recv() for _ in packets] == packets
                second.close()
                wait(lambda: not mapped(process, NAMES[1]))
                publishers[NAMES[0]].send(b"still-connected")
                assert first.recv() == b"still-connected"
                with wire.socket(zmq.SUB) as reconnected:
                    reconnected.setsockopt(zmq.LINGER, 0)
                    reconnected.setsockopt(zmq.RCVTIMEO, 5000)
                    reconnected.setsockopt(zmq.SUBSCRIBE, b"")
                    reconnected.connect(f"tcp://127.0.0.1:{port(NAMES[1])}")
                    wait(lambda: mapped(process, NAMES[1]))
                    time.sleep(.15)
                    publishers[NAMES[1]].send(b"reconnected")
                    assert reconnected.recv() == b"reconnected"
                first.close()
                wait(lambda: all(not mapped(process, name) for name in NAMES))
                assert process.poll() is None
            del publishers, publisher
        context.term()
        return sha256(b"".join(packets)).hexdigest()


def incoming(binary: Path, output: Path) -> str:
    with tempfile.TemporaryDirectory(prefix="msgq_rust-probe-bridge-", dir="/dev/shm") as namespace:
        os.environ["OPENPILOT_PREFIX"] = Path(namespace).name.removeprefix("msgq_")
        context = msgq.Context()
        with zmq.Context() as wire, wire.socket(zmq.PUB) as publisher:
            publisher.setsockopt(zmq.LINGER, 0)
            publisher.bind(f"tcp://127.0.0.1:{port(NAMES[0])}")
            with running(binary, ["127.0.0.1", "prefix-logMessage-suffix"], output) as process:
                wait(lambda: (Path(namespace) / NAMES[0]).exists())
                assert not (Path(namespace) / NAMES[1]).exists()
                subscriber = msgq.SubSocket()
                subscriber.connect(context, NAMES[0], segment_size=SERVICE_LIST[NAMES[0]].queue_size)
                subscriber.setTimeout(5000)
                time.sleep(.4)
                packets = [number.to_bytes(4, "little") + b"incoming\x00\xff" for number in range(160)]
                for packet in packets:
                    publisher.send(packet)
                assert [subscriber.receive() for _ in packets] == packets
                del subscriber
                assert process.poll() is None
        context.term()
        return sha256(b"".join(packets)).hexdigest()


def check(original: Path, binary: Path, output: Path) -> None:
    records = {}
    for label, executable in (("source", original), ("rust", binary)):
        values = {}
        for scenario in (outgoing, incoming):
            destination = output / label / scenario.__name__
            destination.mkdir(parents=True)
            values[scenario.__name__] = scenario(executable, destination)
        records[label] = values
    assert records["source"] == records["rust"]
    (output / "result.json").write_text(json.dumps(records, indent=2) + "\n")
    print("PASS: source/native bridge bidirectional bursts, substring whitelist, client disconnect/reconnect and shutdown")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--original", required=True, type=Path)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    check(args.original.resolve(), args.binary.resolve(), args.output.resolve())
