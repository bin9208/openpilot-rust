#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Compares the native report with caller-provided retained original synthetic captures.
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from typing import TypedDict

type Json = None | bool | int | float | str | list[Json] | dict[str, Json]


class Request(TypedDict, total=False):
    operation: str
    root: str
    route: str
    prefer_rlog: bool
    file: str
    value: float


@dataclass(frozen=True, slots=True)
class Pair:
    name: str
    request: Request
    expected: Json
    keys: tuple[str, ...] = ()


def save(path: Path, value: Json) -> None:
    path.write_text(json.dumps(value, ensure_ascii=True, allow_nan=True, indent=2) + '\n')


def pairs(captures: Path, state: Path, open_stream: Path) -> list[Pair]:
    route = '2026-01-02--03-04-05'
    result = []
    source = json.loads((captures/'receipt.json').read_text())['source']
    assert source['report'] == hashlib.sha256(Path('openpilot/selfdrive/carrot/server/features/dashcam/report.py').read_bytes()).hexdigest()
    for directory in sorted(captures.iterdir()):
        if not directory.is_dir() or directory.name in ('cross-segment', 'preference'): continue
        row = json.loads((directory/'result.json').read_text())
        file = Path(row['file']); assert hashlib.sha256(file.read_bytes()).hexdigest() == row['input_sha256']
        keys = ('ok', 'bytes', 'sha256') if row['decompress']['ok'] else ('ok',)
        result.append(Pair(directory.name+'/codec', {'operation': 'codec', 'file': str(file)}, row['decompress'], keys))
        result.append(Pair(directory.name+'/report', {'operation': 'report', 'root': str(directory),
            'route': route, 'prefer_rlog': True}, row['report']))
    result.append(Pair('cross-segment', {'operation': 'report', 'root': str(captures/'cross-segment'),
        'route': route+'--0', 'prefer_rlog': True}, json.loads((captures/'cross-segment/result.json').read_text())))
    preferred = json.loads((captures/'preference/result.json').read_text())
    for name in ('rlog', 'qlog'):
        result.append(Pair('preference/'+name, {'operation': 'report', 'root': str(captures/'preference'),
            'route': route, 'prefer_rlog': name == 'rlog'}, preferred[name]))
    formats = json.loads((captures/'format.json').read_text())
    for field in ('hms', 'ms', 'clock'):
        for value, expected in formats[field]:
            result.append(Pair(f'format/{field}/{value}', {'operation': 'format', 'value': value}, {field: expected}, (field,)))
    for value, round2, round1 in formats['decimal']:
        result.append(Pair(f'format/decimal/{value}', {'operation': 'format', 'value': value},
            {'round2': round2, 'round1': round1}, ('round2', 'round1')))
    for name in ('state-carry', 'excursion-gap', 'excursion-cap'):
        row = json.loads((state/name/'result.json').read_text())
        for item in row['inputs']: assert hashlib.sha256(Path(item['file']).read_bytes()).hexdigest() == item['sha256']
        result.append(Pair(name, {'operation': 'report', 'root': str(state/name), 'route': route,
            'prefer_rlog': True}, row['report']))
    row = json.loads((open_stream/'result.json').read_text())
    file = Path(row['file']); assert hashlib.sha256(file.read_bytes()).hexdigest() == row['input_sha256']
    result.append(Pair('open-stream/codec', {'operation': 'codec', 'file': str(file)}, row['decompress'], ('ok', 'bytes', 'sha256')))
    result.append(Pair('open-stream/report', {'operation': 'report', 'root': str(open_stream),
        'route': route, 'prefer_rlog': True}, row['report']))
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--source-captures', type=Path, required=True)
    parser.add_argument('--state-captures', type=Path, required=True)
    parser.add_argument('--open-stream-captures', type=Path, required=True)
    parser.add_argument('--only-zstd', action='store_true')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args(); output = args.output.resolve(); output.mkdir(parents=True)
    cases = pairs(args.source_captures.resolve(), args.state_captures.resolve(), args.open_stream_captures.resolve())
    if args.only_zstd: cases = [case for case in cases if case.name.startswith(('zstd', 'open-stream'))]
    data = ''.join(json.dumps(case.request) + '\n' for case in cases)
    command = [str(args.binary.resolve())]
    invocation = {'command': [sys.executable, '-P', *sys.argv], 'native_command': command,
        'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'TZ': os.environ.get('TZ'),
        'codec_error_comparison': 'success/bytes/digest or failed; internal decoder message not HTTP-visible'}
    result = subprocess.run(command, input=data, text=True, capture_output=True, timeout=20)
    (output/'native-input.jsonl').write_text(data); (output/'native-output.jsonl').write_text(result.stdout)
    (output/'native-stderr.log').write_text(result.stderr); invocation['exit'] = result.returncode
    save(output/'invocation.json', invocation)
    assert result.returncode == 0, result.stderr
    actual = [json.loads(line) for line in result.stdout.splitlines()]; assert len(actual) == len(cases)
    rows = []; differences = []
    for case, value in zip(cases, actual, strict=True):
        expected = {key: case.expected[key] for key in case.keys} if case.keys else case.expected
        selected = {key: value.get(key) for key in case.keys} if case.keys else value
        same = json.dumps(expected, sort_keys=True, allow_nan=True) == json.dumps(selected, sort_keys=True, allow_nan=True)
        rows.append({'name': case.name, 'equal': same, 'expected': expected, 'native': selected})
        if not same: differences.append(case.name)
    save(output/'comparison.json', {'cases': rows, 'count': len(rows), 'differences': differences})
    print(json.dumps({'count': len(rows), 'differences': differences}))
    assert not differences


if __name__ == '__main__':
    main()
