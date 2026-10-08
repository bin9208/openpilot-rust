#!/usr/bin/env python3
"""Force the original error publisher to initialize after the QA subscriber."""
from __future__ import annotations

import argparse
from contextlib import ExitStack, contextmanager
from hashlib import sha256
import json
import os
from pathlib import Path
import select
import subprocess
import sys

import openpilot.cereal.messaging as messaging
import check_managed_entry
from logmessaged_native import Peer
from logmessaged_reference import ROOT, run_original


@contextmanager
def replace(target, name, value):
    previous = getattr(target, name)
    setattr(target, name, value)
    try:
        yield
    finally:
        setattr(target, name, previous)


def signal_read(fd: int) -> None:
    assert select.select([fd], [], [], 10)[0], 'publisher initialization gate timed out'
    assert os.read(fd, 1) == b'1', 'publisher initialization gate closed'


def collector(endpoint: str, root: Path, release: int, acknowledged: int) -> None:
    original_publish = messaging.pub_sock
    queue_owner = None

    def publish(name, *args, **kwargs):
        nonlocal queue_owner
        if name == 'errorLogMessage':
            # A real subscriber creates the queue without initializing its publisher.
            queue_owner = messaging.sub_sock(name)
            signal_read(release)
        result = original_publish(name, *args, **kwargs)
        if name == 'errorLogMessage':
            os.write(acknowledged, b'1')
        return result

    with replace(messaging, 'pub_sock', publish):
        run_original(endpoint, root)


class GatedPeer(Peer):
    def start(self):
        original_popen = subprocess.Popen
        original_subscribe = messaging.sub_sock
        release_read, release_write = os.pipe()
        acknowledged_read, acknowledged_write = os.pipe()
        gate = {'subscriber_created': False, 'publisher_initialized_after_subscriber': False}

        def launch(command, **kwargs):
            assert self.original
            assert command[1] == str(ROOT / 'rust/tools/logmessaged_reference.py')
            return original_popen(
                [sys.executable, str(Path(__file__).resolve()), '--collector',
                 self.endpoint, str(self.root), str(release_read), str(acknowledged_write)],
                pass_fds=(release_read, acknowledged_write), **kwargs)

        def subscribe(name, *args, **kwargs):
            result = original_subscribe(name, *args, **kwargs)
            if name == 'errorLogMessage':
                gate['subscriber_created'] = True
                os.write(release_write, b'1')
                signal_read(acknowledged_read)
                gate['publisher_initialized_after_subscriber'] = True
                (self.output / 'initialization-order.json').write_text(json.dumps(gate) + '\n')
            return result

        with ExitStack() as stack:
            for fd in (release_read, release_write, acknowledged_read, acknowledged_write):
                stack.callback(os.close, fd)
            stack.enter_context(replace(subprocess, 'Popen', launch))
            stack.enter_context(replace(messaging, 'sub_sock', subscribe))
            super().start()
        assert all(gate.values())

    def close(self):
        if self.process is not None:
            queue = self.shm / 'errorLogMessage'
            raw = queue.read_bytes() if queue.is_file() else b''
            (self.output / 'queue-diagnostic.json').write_text(json.dumps({
                'collector_returncode_before_close': self.process.poll(),
                'error_queue_contains_crash': b'"crash"' in raw,
                'error_queue_sha256': sha256(raw).hexdigest(),
                'namespace': self.prefix,
            }) + '\n')
        super().close()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('collector', type=Path)
    parser.add_argument('binding', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    with replace(check_managed_entry, 'Peer', GatedPeer):
        result = check_managed_entry.run(
            args.binary.resolve(), args.collector.resolve(), args.binding.resolve(),
            output / 'reset-error', {'body': 'return', 'reset': 'error'}, 'source', True, [])
    assert result['response']['outcome']['kind'] == 'raised'
    assert len(result['records']['errorLogMessage']) == 1
    assert result['records']['errorLogMessage'] == result['records']['logMessage']
    manifest = {'result': 'PASS', 'scenario': 'reset-error',
                'schedule': 'subscriber-before-publisher', 'error_messages': 1,
                'source_sha256': {name: sha256((ROOT / name).read_bytes()).hexdigest() for name in (
                    'msgq_repo/msgq/msgq.cc', 'openpilot/system/logmessaged.py',
                    'rust/tools/check_managed_entry.py', 'rust/tools/logmessaged_native.py')}}
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(manifest))


if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--collector':
        collector(sys.argv[2], Path(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5]))
    else:
        main()
