#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# Use the retained original aiohttp environment; no installation is needed.
# python -P rust/tools/carrot_server_dashcam_catalog.py --binary PATH --output DIR
# ──────────────────
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import sys
from types import ModuleType, SimpleNamespace
from typing import TypeAlias

from aiohttp import web

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']

def save(path: Path, data: Json) -> None:
  path.write_text(json.dumps(data, ensure_ascii=True, indent=2, allow_nan=True) + '\n')

def source_modules():
  base = Path('openpilot/selfdrive/carrot/server/features').resolve()
  for name, directory in (('features', base), ('features.dashcam', base / 'dashcam')):
    module = ModuleType('openpilot.selfdrive.carrot.server.' + name); module.__path__ = [str(directory)]; sys.modules[module.__name__] = module
  from openpilot.selfdrive.carrot.server.features.dashcam import catalog, paths, read_state
  return catalog, paths, read_state

def normalized(data: Json, roots: list[Path]) -> Json:
  match data:
    case str():
      for root in roots: data = data.replace(str(root), '$OWNED')
      return data
    case list(): return [normalized(value, roots) for value in data]
    case dict(): return {key: normalized(value, roots) for key, value in data.items()}
    case None | bool() | int() | float(): return data
  raise RuntimeError('unhandled fixture JSON type')

def snapshot(path: Path) -> dict[str, Json]:
  result: dict[str, Json] = {}
  for name, candidate in (('state', path), ('temporary', Path(str(path) + '.tmp'))):
    result[name] = {'kind': 'file', 'body_base64': base64.b64encode(candidate.read_bytes()).decode()} if candidate.is_file() else {'kind': 'directory' if candidate.is_dir() else 'missing'}
  return result

def main() -> None:
  parser = argparse.ArgumentParser(); parser.add_argument('--binary', type=Path, required=True); parser.add_argument('--output', type=Path, required=True); args = parser.parse_args()
  output = args.output.resolve(); output.mkdir(parents=True, exist_ok=True); binary = args.binary.resolve()
  free = shutil.disk_usage(output).free; save(output / 'diskguard.json', {'free_bytes': free, 'estimated_growth_bytes': 8 * 1024**2, 'reserve_bytes': 25 * 1024**3})
  if free < 25 * 1024**3 + 8 * 1024**2: raise RuntimeError('disk reserve requires recovery before fixture generation')
  save(output / 'invocation.json', {'command': [sys.executable, '-P', *sys.argv], 'PYTHONPATH': os.environ.get('PYTHONPATH', ''), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()})
  source, paths, state = source_modules()
  source_files = [Path('openpilot/selfdrive/carrot/server/features/dashcam') / name for name in ('catalog.py', 'paths.py', 'read_state.py')]
  save(output / 'source-hashes.json', {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in source_files})
  root = output / 'owned-segments'; root.mkdir()
  caches = [output / name for name in ('original-cache', 'native-cache')]
  state_paths = [output / name / 'state.json' for name in ('original-state', 'native-state')]
  source.DASHCAM_ROOT = paths.DASHCAM_ROOT = str(root); paths.DASHCAM_CACHE_DIR = str(caches[0]); state.CARROT_DASHCAM_READ_STATE_PATH = str(state_paths[0]); source._end_epoch_cache = {}
  source_names = ['0000000a--aaaaaaaaaa--' + suffix for suffix in ('0', '1', '2', '3', '4')] + ['0000000b--1111111111--0', '0000000a--fffffffffe--0', '2026-10-08--12-34-00--2', '2026-10-08--12-34-00--02', 'r--²', 'r--１', 'invalid', 'r--bad']
  for name in source_names: (root / name).mkdir()
  (root / 'file--1').write_bytes(b'owned ordinary file'); (root / 'link--1').symlink_to(root / source_names[0], target_is_directory=True)
  def file(directory: Path, name: str, content: bytes, epoch: float = 100.) -> Path:
    directory.mkdir(exist_ok=True); path = directory / name; path.write_bytes(content); os.utime(path, (epoch, epoch)); return path
  for index, epoch in ((0,100.), (1,165.), (3,300.), (4,290.)): file(root / source_names[index], 'qcamera.ts', b'owned synthetic video marker', epoch)
  file(root / source_names[2], 'qcamera.ts', b'', 200.)
  media = root / 'media--0'; file(media, 'qcamera.ts', b'owned TS', 101.75); file(media, 'qcamera.mp4', b'owned MP4', 800.9)
  for name in ('rlog.zst', 'rlog.bz2', 'rlog', 'qlog.zst', 'qlog.bz2', 'qlog'): file(media, name, b'owned metadata-only log marker')
  target = file(root / 'target--0', 'owned-target', b'owned symlink target', 999.)
  links = root / 'symlinks--0'; links.mkdir(); (links / 'qcamera.ts').symlink_to(target); os.utime(links / 'qcamera.ts', (123.,123.), follow_symlinks=False); (links / 'rlog.zst').symlink_to(target)
  overwrite = root / 'overwrite--0'; file(overwrite, 'rlog.zst', b'owned good'); file(overwrite, 'rlog.bz2', b'')
  reversed_order = root / 'reversed--0'; file(reversed_order, 'rlog.bz2', b''); file(reversed_order, 'rlog.zst', b'owned good')
  locked = root / 'locked--0'; file(locked, 'rlog.zst', b'owned good'); (locked / 'directory.lock').mkdir()
  complete = root / 'complete--0'; file(complete, 'rlog.zst', b'owned good')
  absent = root / 'absent--0'; absent.mkdir()
  save(output / 'filesystem-entry-order.json', {directory.name: [entry.name for entry in os.scandir(directory)] for directory in (overwrite, reversed_order, locked, links)})
  config = {'root': str(root), 'cache': str(caches[1]), 'state': str(state_paths[1])}; native = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
  native.stdin.write(json.dumps(config) + '\n'); native.stdin.flush()
  observations = []; failures = []
  def original(case):
    operation = case['operation']; paths.time = state.time = SimpleNamespace(time=lambda: case.get('now',1000.))
    try:
      match operation:
        case 'routes': value = source.build_routes()
        case 'complete': value = source.segment_is_complete(case['segment'])
        case 'invalidate': value = source.invalidate_segment_time_cache()
        case 'times': value = source.compute_segment_times(case['segments'], case.get('seed'))
        case 'bounds': value = source.route_time_bounds(case['segments'])
        case 'end_epoch': value = source.source_video_end_epoch(case['directory'])
        case 'video': value = source.source_video(case['directory'])
        case 'rlog': value = source.source_rlog(case['directory'])
        case 'qlog': value = source.source_qlog(case['directory'])
        case 'summary': value = source.segment_file_summary(case['directory'])
        case 'route_key': value = source.route_creation_key(case['value'])
        case 'segment_key': value = source.segment_creation_key(case['value'])
        case 'safe_segment': value = paths.safe_segment(case['value'])
        case 'index': value = paths.segment_index(case['value'])
        case 'route_name': value = paths.route_name(case['value'])
        case 'date': value = paths.route_date_label(case['value'])
        case 'size': value = paths.file_size_label(case['value'])
        case 'relative': value = paths.relative_time(case['epoch'])
        case 'segment_dir': value = paths.segment_dir(case['value'])
        case 'cache_path': value = paths.cache_path(case['kind'], case['value'], case['extension'])
        case 'normalize': value = state.normalize_recent_segment(case['value'])
        case 'read': value = state.read_dashcam_read_state()
        case 'write': value = state.write_dashcam_recent_segment(case['value'])
        case _: raise RuntimeError('unhandled fixture operation')
      return json.loads(json.dumps({'value': value}, ensure_ascii=True))
    except (web.HTTPException, ValueError, TypeError, OverflowError, OSError, UnicodeError) as error:
      return {'error': error.text if isinstance(error, web.HTTPException) else str(error), 'status': error.status if isinstance(error, web.HTTPException) else None, 'valueError': isinstance(error, ValueError)}
  def pair(operation: str, scenario: str, **fields: Json) -> None:
    case = dict(fields, operation=operation); expected = original(case)
    native.stdin.write(json.dumps(case, ensure_ascii=True, allow_nan=True) + '\n'); native.stdin.flush()
    if not select.select([native.stdout], [], [], 5.)[0]: raise RuntimeError('native fixture response timeout')
    actual = json.loads(native.stdout.readline()); row = {'scenario': scenario, 'input': case, 'original': expected, 'native': actual}
    if operation in ('read', 'write'):
      row['original_files'] = snapshot(state_paths[0]); row['native_files'] = snapshot(state_paths[1])
    equal = normalized(expected,[caches[0],state_paths[0].parent]) == normalized(actual,[caches[1],state_paths[1].parent])
    if 'original_files' in row: equal = equal and row['original_files'] == row['native_files']
    row['equal'] = equal; observations.append(row)
    if not equal: failures.append(row)
  pair('routes', 'name-only-numeric-modern-legacy-and-digit-class-index-order')
  trace_command = ['/usr/bin/strace', '-f', '-e', 'trace=%file,getdents64', '-s', '4096', '-o', str(output / 'native-name-only.strace'), str(binary)]
  traced = subprocess.run(trace_command, input=json.dumps(config) + '\n' + '{"operation":"routes"}\n\n', text=True, capture_output=True, timeout=10)
  trace = (output / 'native-name-only.strace').read_text(); child_lines = [line for line in trace.splitlines() if str(root) + '/' in line]
  save(output / 'name-only-trace.json', {'command': trace_command, 'returncode': traced.returncode, 'stdout': traced.stdout, 'stderr': traced.stderr, 'child_file_accesses': child_lines})
  if traced.returncode or child_lines: raise RuntimeError('name-only enumeration accessed child metadata')
  for value in (' r--1 ', '', '.', 'r--²', 'r--１', 'r--1/x', 'r\\--1'):
    pair('safe_segment', 'segment-safety-and-digit-class', value=value)
  for value in ('r--²', 'r--１', 'r--bad', 'r--2_0'): pair('index', 'isdigit-int-distinction', value=value)
  for value in ('0000000a--AAAAAAAAAA', '0000000b--1111111111', '2026-10-08--12-34-00', None): pair('route_key', 'modern-legacy-key', value=value)
  for value in (source_names[0], source_names[2], 'r--²', None): pair('segment_key', 'segment-numeric-key', value=value)
  for value in ('route--1', None): pair('route_name', 'route-name', value=value)
  for value in ('2026-10-08--12-34-00', '20261008--123400', 'abc--tag', None): pair('date', 'source-date-label-branches', value=value)
  for value in (0,1023,1024,1048576,1073741824,'-1.25','bad',float('nan'),float('inf'),float('-inf')): pair('size', 'size-boundary-or-conversion', value=value)
  for value in (None,0,False,[],['r--1'],'bad',' r--１ ','\ud800--1'): pair('normalize', 'read-state-normalize', value=value)
  pair('relative','shared-relative-representative',epoch=941,now=1000)
  pair('segment_dir','actual-segment-directory',value=' ' + source_names[0] + ' '); pair('segment_dir','missing-segment-directory',value='missing--1')
  pair('cache_path','actual-cache-token-and-mkdir',value=' raw 차량 ',kind='thumb',extension='.jpg')
  for operation in ('video','rlog','qlog','end_epoch','summary'): pair(operation, 'canonical-file-preference', directory=str(media))
  (media / 'rlog.zst').write_bytes(b''); pair('rlog', 'empty-preferred-log-fallback', directory=str(media)); pair('summary','one-recorded-source-per-kind',directory=str(media))
  for operation in ('video','rlog','qlog','summary'): pair(operation,'missing-required-source',directory=str(absent))
  for operation in ('video','end_epoch','summary'): pair(operation,'follow-target-size-nofollow-epoch',directory=str(links))
  for directory in (overwrite,reversed_order,locked,links,absent,complete): pair('complete','per-entry-overwrite-and-lock',segment=directory.name)
  pair('times','ascending-contiguous-and-missing',segments=source_names[:3]); pair('times','preceding-page-seed',segments=source_names[1:3],seed=source_names[0]); pair('bounds','first-last-only',segments=[source_names[0],source_names[1],source_names[4]])
  os.utime(root / source_names[0] / 'qcamera.ts',(400.,400.)); pair('bounds','positive-cache-retains-old-time',segments=[source_names[0]])
  file(root / source_names[2],'qcamera.ts',b'owned completed marker',230.); pair('times','zero-not-cached-recovery',segments=source_names[1:3])
  os.utime(root / source_names[2] / 'qcamera.ts',(500.,500.)); pair('bounds','new-positive-time-now-cached',segments=[source_names[2]])
  pair('invalidate','explicit-time-cache-invalidation'); pair('times','invalidation-gap-backward-time',segments=source_names[:5]); pair('bounds','empty-route-bounds',segments=[])
  root.chmod(0); pair('routes','actual-directory-error'); root.chmod(0o700); pair('routes','directory-error-recovery')
  pair('read','missing-read-state')
  for path in state_paths: path.parent.mkdir()
  for raw in ('[]', '{"recentSegment":" r--１ "}', '{"recentSegment":["r--1"]}', '{broken'):
    for path in state_paths: path.write_text(raw)
    pair('read','existing-invalid-or-nonscalar-state')
  for value in (' 2026-10-08--차량--1 ', None, 'bad', '\ud800--1', 'valid--２'):
    pair('write','state-utf8-partial-temp-error-recovery',value=value,now=1000)
  for path in state_paths: path.unlink(); path.mkdir()
  pair('write','actual-replace-destination-directory-error',value='owned--1',now=1001)
  for path in state_paths: path.rmdir()
  pair('write','replace-error-recovery',value='owned--2',now=1002); pair('read','read-written-state')
  native.stdin.write('\n'); native.stdin.flush(); stdout,stderr = native.communicate(timeout=10); (output / 'native-stdout.txt').write_text(stdout); (output / 'native-stderr.txt').write_text(stderr)
  save(output / 'observations.json', observations); save(output / 'failures.json',failures); save(output / 'result.json',{'pairs':len(observations),'failures':len(failures),'native_exit_code':native.returncode,'name_only_child_accesses':len(child_lines),'limits':['owned metadata-only synthetic files; no decoding, encoding, HTTP, upload, device or NAS','relative-time full boundary proof and inherited Value/JSON corpus reused']})
  if failures or native.returncode: raise SystemExit(1)

if __name__ == '__main__': main()
