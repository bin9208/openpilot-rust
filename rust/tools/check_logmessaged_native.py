# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "pyzmq==27.2.0", "zstandard==0.25.0"]
# ///
"""Run: PYTHONPATH=<built-msgq>:.:rust/tools python rust/tools/check_logmessaged_native.py --help."""
from __future__ import annotations

import argparse
from hashlib import sha256
import json
from pathlib import Path
import signal
import subprocess
import time

from logmessaged_native import Peer
from logmessaged_reference import file_snapshot


def fixtures():
    for level in (0, 10, 19, 20, 39, 40, 50, 255):
        data = json.dumps({'msg': {'level': level, 'n': 181449526438435.12, 's': '한😀'}, 'ctx': {'fixed': True}}, ensure_ascii=False).encode()
        yield f'level-{level}', [b'', bytes([level]) + data[:13], data[13:14], b'', data[14:]]
    yield 'invalid-json', [bytes([40]), b'{not JSON']
    yield 'invalid-utf8', [bytes([20]), b'{"msg":"a\xff\xed\xa0\x80\xe2', b'\x82"}']
    yield 'nul', [bytes([40]), b'{"msg":"a\x00b"}']
    yield 'unicode-large', [bytes([40]), json.dumps({'msg': '😀' * 600000}, ensure_ascii=False).encode()]
    for delta in (-1, 0, 1):
        data = json.dumps({'msg': 'x' * (2 * 1024 * 1024 - 11 + delta)}).encode()
        assert len(data) == 2 * 1024 * 1024 + delta
        yield f'limit-{delta}', [bytes([40]), data]
    yield 'last', [bytes([20]), b'{"msg":"finished"}']


def compare_case(binary: Path, output: Path, name: str, records, seed=None, delay=False, resource_difference=False):
    destination = output / name
    peers = [Peer(binary, destination / side, original=side == 'original', frames=2 * len(records)) for side in ('original', 'rust')]
    try:
        if seed is not None:
            for peer in peers:
                seed(peer.root)
        for peer in peers:
            peer.start()
        for index, (tag, parts) in enumerate(records):
            if delay and index:
                time.sleep(61.)
            for peer in peers:
                peer.send(parts, tag)
        # Rust's optional bound counts input records, including dropped records.
        code = peers[1].process.wait(timeout=10)
        assert code == (1 if name == 'write-error' else 0), (name, code)
        peers[0].stop()
        assert peers[0].records == peers[1].records, name
        original, rust = [file_snapshot(peer.root) for peer in peers]
        if resource_difference:
            assert len(original) == len(rust) == 1
            assert next(iter(original.values())) == {'content': ''}
            assert '"a$i": 0' in next(iter(rust.values()))['content']
            assert 'RecursionError' in (peers[0].output / 'stderr.log').read_text()
        else:
            assert original == rust, (name, original.keys(), rust.keys())
        assert peers[0].capacities == peers[1].capacities
        summary = {'records': len(records), 'log_messages': len(peers[1].records['logMessage']),
                   'error_messages': len(peers[1].records['errorLogMessage']), 'files': len(rust),
                   'queue_file_bytes': peers[1].capacities, 'rust_exit': code, 'cpython_recursion_boundary_difference': resource_difference,
                   'file_sha256_after_uuid_normalization': {key: sha256(json.dumps(value, sort_keys=True).encode()).hexdigest() for key, value in rust.items()}}
        (destination / 'report.json').write_text(json.dumps(summary, indent=2) + '\n')
        return {key: value for key, value in summary.items() if key != 'file_sha256_after_uuid_normalization'}
    finally:
        for peer in peers:
            peer.close()


def errors(binary: Path, output: Path):
    results = {}
    for signum in (signal.SIGINT, signal.SIGTERM):
        peer = Peer(binary, output / f'idle-{signum}', False)
        try:
            peer.start()
            time.sleep(.15)
            assert peer.process.poll() is None
            code, elapsed = peer.stop(signum)
            assert code == 0 and elapsed < .5, (signum, code, elapsed)
            results[str(signum)] = {'exit': code, 'shutdown_seconds': elapsed}
        finally:
            peer.close()
    for original in (True, False):
        peer = Peer(binary, output / f'empty-{original}', original)
        try:
            peer.start()
            peer.socket.send_multipart([b'', b''])
            code = peer.process.wait(timeout=10)
            assert code != 0
            assert peer.subscribers['logMessage'].receive(non_blocking=True) is None
            results[f'empty-{original}'] = {'exit': code}
        finally:
            peer.close()
    for arguments in (['--frames', '0'], ['--unknown'], ['--log-root', '/dev/null/impossible']):
        result = subprocess.run([binary, *arguments], capture_output=True, text=True, timeout=10)
        assert result.returncode != 0 and result.stderr
        results[' '.join(arguments)] = {'exit': result.returncode, 'stderr': result.stderr}
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    args.binary = args.binary.resolve()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    def old_files(root):
        for index in range(2502):
            (root / f'swaglog.{index:010}').write_text('old\n')
    def bad_rollover(root):
        (root / 'swaglog.0000000001').mkdir()
    def full_disk(root):
        (root / 'swaglog.0000000000').symlink_to('/dev/full')
    report = {'records-and-retention': compare_case(args.binary, args.output, 'records', list(fixtures()), old_files),
              'rollover-open-error': compare_case(args.binary, args.output, 'rollover-open-error',
                                                [('large', [b'\x14', json.dumps({'msg': 'x' * 300000}).encode()]),
                                                 ('failure', [b'\x28', b'{"msg":"failed rollover"}']),
                                                 ('closed', [b'\x28', b'{"msg":"closed handler"}'])], bad_rollover),
              'write-error': compare_case(args.binary, args.output, 'write-error',
                                         [('first', [b'\x28', b'{"msg":"full"}']), ('second', [b'\x28', b'{"msg":"still full"}'])], full_disk),
              'deep-object-publication': compare_case(args.binary, args.output, 'deep-object-publication',
                    [('deep', [b'\x28', ('{"msg":' + '{"a":' * 1500 + '0' + '}' * 1500 + '}').encode()])], resource_difference=True),
              'time-boundary': compare_case(args.binary, args.output, 'time-boundary',
                                           [('before', [b'\x14', b'{"msg":"before interval"}']),
                                            ('after', [b'\x14', b'{"msg":"after interval"}'])], delay=True),
              'failure-and-idle': errors(args.binary, args.output)}
    (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
