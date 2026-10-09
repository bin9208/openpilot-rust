#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run with caller-supplied full-cereal/zstandard dependencies from repository root.
# python -P rust/tools/carrot_server_dashcam_report_source_controls.py --output NEW_DIR
from __future__ import annotations

import argparse
import ast
import bz2
import hashlib
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace
from typing import TypedDict

import anyio
from aiohttp import web
import zstandard as zstd
from openpilot.cereal import log
from carrot_server_dashcam_catalog import source_modules
from carrot_server_dashcam_upload import save


class Fields(TypedDict, total=False):
    wallTimeNanos: int
    enabled: bool
    active: bool
    vEgo: float
    aEgo: float
    gearShifter: str
    steeringPressed: bool
    standstill: bool
    brakePressed: bool


def event(kind: str, mono: int, fields: Fields) -> bytes:
    message = log.Event.new_message(); message.logMonoTime = mono
    value = message.init(kind)
    for key, field in fields.items(): setattr(value, key, field)
    return message.to_bytes()


def stream(nan_speed: bool = False, nan_accel: bool = False) -> bytes:
    frames = [event('initData', 1_000_000_000, {'wallTimeNanos': 1_700_000_000_000_000_000}),
              event('selfdriveState', 1_000_000_000, {'enabled': True, 'active': True})]
    for index in range(6):
        frames.append(event('carState', 1_000_000_000+index*250_000_000,
            {'vEgo': float('nan') if nan_speed and index == 2 else 9.375,
             'aEgo': float('nan') if nan_accel and index == 2 else (2.675 if index < 3 else -2.675),
             'gearShifter': 'drive', 'steeringPressed': index < 3, 'standstill': index == 4}))
    return b''.join(frames)


def main() -> None:
    parser = argparse.ArgumentParser(); parser.add_argument('--output', type=Path, required=True)
    output = parser.parse_args().output.resolve(); output.mkdir(parents=True)
    catalog, paths, _ = source_modules()
    from openpilot.selfdrive.carrot.server.features.dashcam import report
    route_path = Path('openpilot/selfdrive/carrot/server/features/dashcam/routes.py')
    route_text = route_path.read_text()
    node = next(node for node in ast.parse(route_text).body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'api_dashcam_report')
    api_scope = {'web': web, 'asyncio': sys.modules['asyncio'], 'build_route_report': report.build_route_report}
    exec(compile(ast.Module(body=[node], type_ignores=[]), str(route_path), 'exec'), api_scope)
    route = '2026-01-02--03-04-05'
    payload = stream(); other = stream(nan_accel=True)
    compressed = zstd.ZstdCompressor(write_checksum=True).compress(payload)
    bz = bz2.compress(payload)
    codec_cases = [('raw', 'rlog', payload), ('zstd', 'rlog.zst', compressed), ('bz2', 'rlog.bz2', bz),
                   ('bz2-multiple', 'rlog.bz2', bz+bz), ('bz2-trailing', 'rlog.bz2', bz+b'owned trailing junk'),
                   ('bz2-truncated', 'rlog.bz2', bz[:-5]), ('bz2-corrupt-tail', 'rlog.bz2', bz+b'BZh9bad'),
                   ('zstd-multiple', 'rlog.zst', compressed+compressed), ('zstd-trailing', 'rlog.zst', compressed+b'owned trailing junk'),
                   ('zstd-truncated', 'rlog.zst', compressed[:-5]), ('zstd-corrupt-tail', 'rlog.zst', compressed[:-1]+bytes([compressed[-1]^1])),
                   ('capnp-partial-tail', 'rlog', payload+b'\x01\x00\x00\x00'),
                   ('nan-speed', 'rlog', stream(nan_speed=True)), ('nan-accel', 'rlog', other)]
    rows = []
    for name, filename, data in codec_cases:
        root = output/name; directory = root/(route+'--0'); directory.mkdir(parents=True)
        path = directory/filename; path.write_bytes(data); os.utime(path, (100,100))
        catalog.DASHCAM_ROOT = paths.DASHCAM_ROOT = report.DASHCAM_ROOT = str(root)
        catalog._end_epoch_cache = {}
        try:
            decoded = report._decompress(str(path)); decode = {'ok': True, 'bytes': len(decoded), 'sha256': hashlib.sha256(decoded).hexdigest()}
        except (OSError, EOFError, ValueError, zstd.ZstdError) as error:
            decode = {'ok': False, 'type': type(error).__name__, 'error': str(error)}
        result = report.build_route_report(route)
        serialized = json.dumps(result, ensure_ascii=True, allow_nan=True)
        response = anyio.run(api_scope['api_dashcam_report'], SimpleNamespace(match_info={'route':route}, query={}))
        row = {'name': name, 'file': str(path), 'input_sha256': hashlib.sha256(data).hexdigest(),
               'decompress': decode, 'report': result, 'json': serialized,
               'source_api_status': response.status, 'source_api_body': response.body.decode()}
        save(root/'result.json', row); rows.append(row)
    root = output/'cross-segment'; directory = root/(route+'--0'); directory.mkdir(parents=True)
    (directory/'rlog').write_bytes(payload+b'\x01\x00\x00\x00')
    directory = root/(route+'--1'); directory.mkdir()
    second = event('initData', 2_000_000_000, {'wallTimeNanos': 1_700_000_001_300_000_000})
    second += event('selfdriveState', 2_000_000_000, {'enabled': False, 'active': False})
    second += event('carState', 2_000_000_000, {'vEgo': 5.0, 'aEgo': -3.25, 'gearShifter': 'drive', 'brakePressed': True})
    second += event('carState', 2_500_000_000, {'vEgo': 5.0, 'aEgo': 0.0, 'gearShifter': 'drive'})
    (directory/'qlog').write_bytes(second)
    catalog.DASHCAM_ROOT = paths.DASHCAM_ROOT = report.DASHCAM_ROOT = str(root); catalog._end_epoch_cache = {}
    save(root/'result.json', report.build_route_report(route+'--0'))
    root = output/'preference'; directory = root/(route+'--0'); directory.mkdir(parents=True)
    (directory/'rlog').write_bytes(payload); (directory/'qlog').write_bytes(second)
    catalog.DASHCAM_ROOT = paths.DASHCAM_ROOT = report.DASHCAM_ROOT = str(root); catalog._end_epoch_cache = {}
    save(root/'result.json', {'rlog': report.build_route_report(route), 'qlog': report.build_route_report(route, False)})
    save(output/'format.json', {'hms': [[value, report._fmt_hms(value)] for value in [-.5,.5,1.5,2.5,3600.5]],
        'ms': [[value, report._fmt_ms(value)] for value in [.5,1.5,60.5]],
        'decimal': [[value, round(value,2), round(value,1)] for value in [-2.675,-1.25,1.25,2.675]],
        'clock': [[value,report._fmt_clock(value)] for value in [0,-1,1_700_000_000]]})
    save(output/'receipt.json', {'command': [sys.executable, '-P', *sys.argv], 'codec_cases': len(rows),
        'source': {'report': hashlib.sha256(Path(report.__file__).read_bytes()).hexdigest(),
                   'catalog': hashlib.sha256(Path(catalog.__file__).read_bytes()).hexdigest(),
                   'api_body': hashlib.sha256(ast.get_source_segment(route_text,node).encode()).hexdigest()},
        'scope': 'small original synthetic full-cereal codec/partial-segment/state/format/NaN controls; no native code/build/device logs'})
    print('original report controls recorded')


if __name__ == '__main__':
    main()
