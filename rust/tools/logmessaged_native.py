"""Native original-ZMQ sender and original-msgq/cereal receiver for daemon QA."""
from __future__ import annotations

from hashlib import sha256
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

import zmq
import openpilot.cereal.messaging as messaging
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from logmessaged_reference import ROOT


class Peer:
    def __init__(self, binary: Path, output: Path, original: bool, frames: int | None = None):
        self.output = output
        output.mkdir(parents=True)
        self.prefix = f'log-qa-{os.getpid()}-{output.name}'
        self.shm = Path('/dev/shm') / ('msgq_' + self.prefix)
        self.shm.mkdir()
        self.endpoint = 'ipc:///tmp/logmessage' + self.prefix
        self.root = output / 'home' / ('.comma' + self.prefix) / 'log'
        self.root.mkdir(parents=True)
        self.binary = binary
        self.original = original
        self.frames = frames
        self.process = None
        self.records = {'logMessage': [], 'errorLogMessage': []}
        self.socket = None
        self.subscribers = {}
        self.context = zmq.Context()

    def start(self):
        environment = dict(os.environ, HOME=str(self.output / 'home'), OPENPILOT_PREFIX=self.prefix)
        command = ([sys.executable, str(ROOT / 'rust/tools/logmessaged_reference.py'), self.endpoint, str(self.root)] if self.original
                   else [str(self.binary), *(['--frames', str(self.frames)] if self.frames is not None else [])])
        with (self.output / 'stdout.log').open('wb') as stdout, (self.output / 'stderr.log').open('wb') as stderr:
            self.process = subprocess.Popen(command, env=environment, stdout=stdout, stderr=stderr)
        deadline = time.monotonic() + 20
        while not all((self.shm / name).exists() for name in self.records):
            if self.process.poll() is not None:
                raise RuntimeError((command, self.process.returncode, (self.output / 'stderr.log').read_text()))
            assert time.monotonic() < deadline, command
            time.sleep(.01)
        os.environ['OPENPILOT_PREFIX'] = self.prefix
        self.capacities = {name: (self.shm / name).stat().st_size for name in self.records}
        assert all(size > SERVICE_LIST[name].queue_size for name, size in self.capacities.items())
        self.subscribers = {name: messaging.sub_sock(name, timeout=10000) for name in self.records}
        self.socket = self.context.socket(zmq.PUSH)
        self.socket.setsockopt(zmq.LINGER, 0)
        self.socket.setsockopt(zmq.SNDTIMEO, 10000)
        self.socket.connect(self.endpoint)
        invocation = {'argv': command, 'HOME': environment['HOME'], 'OPENPILOT_PREFIX': self.prefix,
                      'binary_sha256': None if self.original else sha256(self.binary.read_bytes()).hexdigest()}
        (self.output / 'invocation.json').write_text(json.dumps(invocation))

    def synchronize(self):
        assert self.frames is None, 'readiness records consume bounded collector frames'
        deadline = time.monotonic() + 20
        sequence = 0
        while time.monotonic() < deadline:
            assert self.process.poll() is None, (self.process.returncode, (self.output / 'stderr.log').read_text())
            sequence += 1
            marker = json.dumps({'msg': f'ready-{self.prefix}-{sequence}'})
            self.socket.send_multipart([bytes([10]), marker.encode()])
            retry = min(deadline, time.monotonic() + .05)
            while time.monotonic() < retry:
                packet = self.subscribers['logMessage'].receive(non_blocking=True)
                if packet is None:
                    time.sleep(.001)
                    continue
                with (self.output / 'readiness.bin').open('ab') as stream:
                    stream.write(packet)
                with log.Event.from_bytes(packet) as event:
                    assert event.valid and event.which() == 'logMessage'
                    if event.logMessage != marker:
                        continue
                # Publisher construction resets readers; reconnect the error reader before test records arrive.
                assert self.subscribers['errorLogMessage'].receive(non_blocking=True) is None
                (self.output / 'readiness.json').write_text(json.dumps({'acknowledged_marker': marker, 'attempts': sequence}))
                return
        raise TimeoutError('collector did not acknowledge readiness')

    def send(self, parts: list[bytes], tag: str):
        raw = b''.join(parts)
        (self.output / f'{tag}.input').write_bytes(raw)
        before = time.monotonic_ns()
        self.socket.send_multipart(parts)
        level, record = raw[0], raw[1:].decode(errors='replace')
        expected = []
        if len(record) <= 2 * 1024 * 1024:
            expected.append('logMessage')
            if level >= 40:
                expected.append('errorLogMessage')
        for name in expected:
            packet = self.subscribers[name].receive()
            assert packet is not None, (tag, name, self.process.poll(), (self.output / 'stderr.log').read_text()[-500:])
            after = time.monotonic_ns()
            with log.Event.from_bytes(packet) as event:
                assert event.valid and event.which() == name and getattr(event, name) == record, (tag, name)
                assert before - 1000 <= event.logMonoTime <= after + 1000, (before, event.logMonoTime, after)
                value = event.to_dict()
                value.pop('logMonoTime')
            self.records[name].append(value)
            with (self.output / (name + '.bin')).open('ab') as stream:
                stream.write(packet)
        # A debug-level barrier proves every prior multipart record was consumed.
        marker = json.dumps({'msg': 'barrier-' + tag}).encode()
        self.socket.send_multipart([bytes([10]), marker])
        packet = self.subscribers['logMessage'].receive()
        assert packet is not None
        with log.Event.from_bytes(packet) as event:
            assert event.logMessage == marker.decode(), (tag, 'unexpected publication before barrier')
        assert self.subscribers['errorLogMessage'].receive(non_blocking=True) is None, (tag, 'unexpected error publication')

    def stop(self, signum=signal.SIGINT):
        if self.process is None:
            return None
        started = time.monotonic()
        if self.process.poll() is None:
            self.process.send_signal(signum)
        result = self.process.wait(timeout=10)
        elapsed = time.monotonic() - started
        (self.output / 'exit.json').write_text(json.dumps({'code': result, 'shutdown_seconds': elapsed, 'signal': int(signum)}))
        return result, elapsed

    def close(self):
        if self.process is not None and self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=5)
        if self.socket is not None:
            self.socket.close()
        self.context.term()
        self.subscribers.clear()
        shutil.rmtree(self.shm)
        Path(self.endpoint.removeprefix('ipc://')).unlink(missing_ok=True)
