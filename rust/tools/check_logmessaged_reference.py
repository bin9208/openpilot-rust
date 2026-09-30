# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["numpy==2.5.3"]
# ///
"""Run original formatter and rotating handler against the Rust file probe."""
from __future__ import annotations

import argparse
from contextlib import redirect_stderr
import io
import json
from pathlib import Path
import random
import struct
import subprocess

from logmessaged_reference import SwagLogFileFormatter, file_snapshot, normalize_uuid, rotating_class


def records():
    special = ['null', 'true', '123', '"text"', '[]', '{}', '{', '{"msg":01}', '{"msg":1} trailing',
               '{"msg":{"x":1,"y":true,"x$i":null}}', '{"msg":1,"msg$i":2,"id":"old","ctx":{}}',
               '{"msg":"\\ud800\\udfff\\ud800x\\udfff"}', '{"msg":[1e400,-1e400,NaN,-0.0,0.0001,1e-5,1e16]}',
               '{"msg":' + '1' * 4301 + '}', '{"msg":' + '[' * 1100 + '0' + ']' * 1100 + '}',
               '{"msg":{"😀":1,"\\ud83d\\ude00":2}}', '{"msg":"\\ud800\\uinvalid"}',
               '{"msg":"\\/\\b\\f\\n\\r\\t\\\\\\\""}', '{"msg":1,"msg":2}',
               '{"msg":{"a":true,"a$b":null,"a":false}}']
    yield from special
    rng = random.Random(20260930)
    strings = ['한글😀', '\x00\x1f\x7f', 'quote"slash\\/', '\ud800', '\udfff', '\ud800\udc00', '']
    for _ in range(6000):
        number = struct.unpack('>d', rng.randbytes(8))[0]
        item = {'s': rng.choice(strings), 'f': number, 'i': rng.randrange(-10**80, 10**80), 'b': bool(rng.randrange(2)),
                'nil': None, 'a': [{'inside': number}, rng.choice(strings)], 'd': {'n': rng.randrange(100), 'f': number}}
        value = {'ctx': {'float': number}, 'msg': item, 'level': 'INFO'}
        yield json.dumps(value)


def compare_format(binary: Path, output: Path):
    formatter = SwagLogFileFormatter(None)
    expected = []
    request = output / 'format-input.jsonl'
    with request.open('w') as stream:
        for record in records():
            stream.write(json.dumps(record) + '\n')
            try:
                expected.append({'record': normalize_uuid(formatter.format(record))})
            except (ValueError, KeyError, TypeError, RecursionError):
                expected.append({'error': True})
    (output / 'format-reference.jsonl').write_text(''.join(json.dumps(value) + '\n' for value in expected))
    actual = output / 'format-actual.jsonl'
    subprocess.run([binary, 'format', request, actual], check=True)
    observed = [json.loads(line) for line in actual.read_text().splitlines()]
    for index, (a, b) in enumerate(zip(expected, observed, strict=True)):
        assert a == b, (index, a, b)
    return len(expected)


def compare_rotation(binary: Path, output: Path):
    cases = {
        'boundaries': {'interval': 60., 'max_bytes': 256, 'backup_count': 4, 'seed': [], 'steps':
                       [(0., {'msg': 'first'}), (59.999, {'msg': 'before'}), (60., {'msg': 'at'}),
                        (60., {'msg': 'a' * 1024}), (60., {'msg': 'after size'}), (120., 'invalid JSON')]},
        'clock-read-order': {'interval': 60., 'max_bytes': 0, 'backup_count': 2500, 'seed': [],
                             'clock_reads': [0., 60., 61., 120.], 'steps': [(60., {'msg': 'first'}), (120., {'msg': 'second'})]},
        'existing-order': {'interval': 60., 'max_bytes': 1, 'backup_count': 2,
                           'seed': ['swaglog.0000000001', 'swaglog.0000000002', 'swaglog.0000000003', 'swaglog.note'],
                           'steps': [(0., {'msg': 'first'}), (1., {'msg': 'second'}), (2., {'msg': 'third'})]},
        'unicode-index': {'interval': 0., 'max_bytes': 0, 'backup_count': 2, 'seed': ['swaglog.٠١٢', 'swaglog.１２３'],
                          'steps': [(0., {'msg': 'unicode indices'}), (100000., {'msg': 'disabled rotation'})]},
        'invalid-numeric-index': {'interval': 60., 'max_bytes': 256, 'backup_count': 2500, 'seed': ['swaglog.²'], 'steps': []},
        'invalid-utf8-sort': {'interval': 60., 'max_bytes': 256, 'backup_count': 2, 'seed': ['swaglog.\udcff', 'swaglog.\ue000'], 'steps': []},
        'rollover-open-error': {'interval': 60., 'max_bytes': 1, 'backup_count': 2500, 'seed': [],
                                'steps': [(0., {'msg': 'first'}), (1., {'msg': 'cannot open'}), (2., {'msg': 'closed stream'})]},
        'disabled-size-close-error': {'interval': 60., 'max_bytes': 0, 'backup_count': 2500, 'seed': [],
                                     'steps': [(0., {'msg': 'full'}), (60., {'msg': 'close fails'})]},
        'write-error': {'interval': 60., 'max_bytes': 256, 'backup_count': 2500, 'seed': [],
                        'steps': [(0., {'msg': 'full'}), (1., {'msg': 'still full'})]},
    }
    reports = {}
    for name, case in cases.items():
        directory = output / name
        directory.mkdir()
        roots = [directory / 'original', directory / 'rust']
        for root in roots:
            root.mkdir()
            for filename in case['seed']:
                (root / filename).write_text('old\n')
            if name == 'rollover-open-error':
                (root / 'swaglog.0000000001').mkdir()
            if name in ('write-error', 'disabled-size-close-error'):
                (root / 'swaglog.0000000000').symlink_to('/dev/full')
        current = [0.]
        clock_reads = iter(case['clock_reads']) if 'clock_reads' in case else None
        original = rotating_class(lambda current=current, clock_reads=clock_reads: current[0] if clock_reads is None else next(clock_reads))
        expected = []
        errors = []
        failed = False
        try:
            handler = original(str(roots[0] / 'swaglog'), interval=case['interval'], max_bytes=case['max_bytes'], backup_count=case['backup_count'])
        except (ValueError, OSError):
            failed = True
        if not failed:
            handler.setFormatter(SwagLogFileFormatter(None))
            handler.handleError = lambda record, errors=errors: errors.append(record)
            for at, value in case['steps']:
                current[0] = at
                before = len(errors)
                with redirect_stderr(io.StringIO()):
                    handler.emit(value if isinstance(value, str) else json.dumps(value))
                expected.append({'emitted': len(errors) == before})
            try:
                handler.close()
            except ValueError:
                expected.append({'closed': True})
            except OSError:
                expected.append({'closed': False})
            else:
                expected.append({'closed': True})
        request = {'directory': str(roots[1]), 'interval': case['interval'], 'max_bytes': case['max_bytes'],
                   'backup_count': case['backup_count'], 'steps': [{'at': at, 'record': value if isinstance(value, str) else json.dumps(value)}
                                                               for at, value in case['steps']]}
        request['clock_reads'] = case.get('clock_reads')
        inputs = directory / 'input.json'
        inputs.write_text(json.dumps(request))
        actual = directory / 'result.jsonl'
        subprocess.run([binary, 'rotate', inputs, actual], check=True)
        observed = [json.loads(line) for line in actual.read_text().splitlines()]
        if failed:
            assert len(observed) == 1 and 'initialization_error' in observed[0], (name, observed)
        else:
            assert observed == expected, (name, observed, expected)
        source_files, rust_files = [file_snapshot(root) for root in roots]
        assert source_files == rust_files, (name, source_files, rust_files)
        (directory / 'reference.json').write_text(json.dumps({'steps': expected, 'files': source_files}, indent=2))
        reports[name] = {'steps': len(case['steps']), 'files': len(rust_files), 'initialization_error': failed}
    return reports


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    report = {'formatter_records': compare_format(args.binary.resolve(), args.output),
              'rotation_cases': compare_rotation(args.binary.resolve(), args.output), 'uuid_only_normalization': True}
    (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
