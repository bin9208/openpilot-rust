#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import asyncio
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from types import ModuleType, SimpleNamespace
from typing import Any
from urllib.parse import quote

import aiohttp
from aiohttp import web

HEADERS = ('content-type', 'content-length', 'cache-control', 'content-disposition', 'etag', 'last-modified', 'accept-ranges', 'content-range', 'allow')
WALL = 1700000200
STAMP_NS = 1700000000123456789

def save(path: Path, value: Any) -> None:
  path.write_text(json.dumps(value, ensure_ascii=True, indent=2) + '\n')

def owned_source() -> tuple[Any, Any, Any]:
  base = Path('openpilot/selfdrive/carrot/server/features').resolve()
  for name, path in (('features', base), ('features.dashcam', base / 'dashcam')):
    package = ModuleType(f'openpilot.selfdrive.carrot.server.{name}'); package.__path__ = [str(path)]; sys.modules[package.__name__] = package
  from openpilot.selfdrive.carrot.server.features.screenrecord import catalog, routes
  from openpilot.selfdrive.carrot.server.features.dashcam import paths
  return catalog, routes, paths

def disk_guard(output: Path, growth: int) -> None:
  free = shutil.disk_usage(output).free
  save(output / 'disk-guard.json', {'free_bytes': free, 'estimated_growth_bytes': growth, 'required_reserve_bytes': 25 * 1024**3})
  if free < 25 * 1024**3 + growth: raise RuntimeError('disk reserve requires recovery before fixture generation')

def native_policy(binary: Path, value: dict[str, Any], env: dict[str, str]) -> dict[str, Any]:
  result = subprocess.run([str(binary)], input=json.dumps(value) + '\n', text=True, capture_output=True, timeout=10, env=env)
  if result.returncode: raise RuntimeError(result.stderr)
  return json.loads(result.stdout)

def wrapper_script() -> str:
  return '''#!/usr/bin/python3
import json, os, pathlib, subprocess, sys
root = pathlib.Path(__file__).parent
mode = (root / 'mode').read_text().strip()
with (root / 'commands.jsonl').open('a') as log: log.write(json.dumps({'mode': mode, 'argv': sys.argv[1:]}) + '\\n')
if mode == 'stderr': print('owned stdout', flush=True); print('owned stderr\\r', file=sys.stderr); sys.exit(7)
if mode == 'stdout': print('owned stdout', flush=True); sys.exit(8)
if mode == 'no-output': sys.exit(0)
if mode == 'empty': pathlib.Path(sys.argv[-1]).write_bytes(b''); sys.exit(0)
result = subprocess.run(['/usr/bin/ffmpeg', *sys.argv[1:]])
if result.returncode == 0 and pathlib.Path(sys.argv[-1]).exists(): os.utime(sys.argv[-1], ns=(1700000000123456789,1700000000123456789))
sys.exit(result.returncode)
'''

async def main() -> None:
  parser = argparse.ArgumentParser(); parser.add_argument('--binary', type=Path, required=True); parser.add_argument('--output', type=Path, required=True); parser.add_argument('--composed-only', action='store_true'); parser.add_argument('--policy-only', action='store_true'); args = parser.parse_args()
  output = args.output.resolve(); output.mkdir(parents=True, exist_ok=True); binary = args.binary.resolve(); disk_guard(output, 16 * 1024**2)
  invocation = {'command': [sys.executable, '-P', *sys.argv], 'PYTHONPATH': os.environ.get('PYTHONPATH', ''), 'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}; save(output / 'invocation.json', invocation)
  catalog, routes, paths = owned_source(); original_hashes = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in [Path('openpilot/selfdrive/carrot/server/features/screenrecord/catalog.py'), Path('openpilot/selfdrive/carrot/server/features/screenrecord/routes.py'), Path('openpilot/selfdrive/carrot/server/features/dashcam/paths.py'), Path('openpilot/selfdrive/carrot/server/features/dashcam/ffmpeg.py')]}; save(output / 'source-hashes.json', original_hashes)
  fixture = output / 'owned-files'; fixture.mkdir(exist_ok=True)
  first, second, alias, missing = [fixture / name for name in ('first', 'second', 'alias', 'missing')]
  first.mkdir(exist_ok=True); second.mkdir(exist_ok=True); alias.symlink_to(first, target_is_directory=True)
  non_directory = fixture / 'not-directory'; non_directory.write_bytes(b'owned')
  directories = [str(path) for path in (first, second, alias, missing, non_directory)]
  for index, name in enumerate(('normal.mp4', '차량 🚗"\\.MP4', 'clip.mkv', 'clip.avi', 'clip.mov', 'clip.ts', 'clip.hevc', '.mp4', 'ignored.txt', 'empty.mp4')):
    path = first / name; path.write_bytes(b'' if name == 'empty.mp4' else bytes(range(64)) + name.encode()); os.utime(path, ns=(STAMP_NS, STAMP_NS if index % 2 == 0 else STAMP_NS + 1000000000))
  (first / 'link.mp4').symlink_to(first / 'normal.mp4'); (first / 'directory.mp4').mkdir(); (second / 'older.mov').write_bytes(b'owned older'); os.utime(second / 'older.mov', (-1.75, -1.75))
  generated = subprocess.run(['/usr/bin/ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-f', 'lavfi', '-i', 'color=c=navy:s=160x120:r=10', '-t', '2', '-c:v', 'mpeg4', str(first / 'synthetic.mp4')], capture_output=True, timeout=15)
  (output / 'synthetic-video-stderr.txt').write_bytes(generated.stderr)
  if generated.returncode: raise RuntimeError('owned synthetic FFmpeg video failed')
  os.utime(first / 'synthetic.mp4', ns=(STAMP_NS, STAMP_NS)); save(output / 'synthetic-video.json', {'command': ['/usr/bin/ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-f', 'lavfi', '-i', 'color=c=navy:s=160x120:r=10', '-t', '2', '-c:v', 'mpeg4', str(first / 'synthetic.mp4')], 'size': (first / 'synthetic.mp4').stat().st_size, 'sha256': hashlib.sha256((first / 'synthetic.mp4').read_bytes()).hexdigest()})
  tools = output / 'owned-bin'; tools.mkdir(); ffmpeg = tools / 'ffmpeg'; ffmpeg.write_text(wrapper_script()); ffmpeg.chmod(0o700); (tools / 'mode').write_text('success')
  caches = [output / name for name in ('original-cache', 'native-cache')]
  for cache in caches: cache.mkdir()
  catalog.SCREEN_RECORDING_DIRS = routes.SCREEN_RECORDING_DIRS = tuple(directories); paths.DASHCAM_CACHE_DIR = str(caches[0]); paths.time = SimpleNamespace(time=lambda: WALL)
  clock = [3. if args.composed_only else 2.999]; routes.time = SimpleNamespace(monotonic=lambda: clock[0]); routes._video_cache = {'time': 0.0, 'videos': []}
  defaults = {'directories': directories, 'cache': str(caches[1]), 'ffmpeg': str(ffmpeg), 'wall': WALL, 'monotonic': clock[0]}
  observations: list[dict[str, Any]] = []; failures: list[dict[str, Any]] = []; env = dict(os.environ)
  if not args.composed_only:
    cases = [{'operation': 'catalog', 'scenario': 'extensions-empty-symlinks-order-dedup'}, *[{'operation': 'relative', 'epoch': WALL - delta, 'wall': WALL, 'scenario': 'relative-boundary'} for delta in (-3, 0, 59, 60, 3599, 3600, 86399, 86400)], {'operation': 'relative', 'epoch': 0, 'wall': WALL, 'scenario': 'nonpositive-relative'}, {'operation': 'file_id', 'path': str(first / '..' / 'first' / 'normal.mp4'), 'scenario': 'absolute-lexical-id'}, {'operation': 'token', 'id': ' whitespace 차량 ', 'scenario': 'raw-cache-token'}]
    for timezone in ('UTC', 'Asia/Seoul'):
      for epoch in (-62135596800, -1, 0, WALL, 253402300799, 9000000000000000000): cases.append({'operation': 'date', 'epoch': epoch, 'TZ': timezone, 'scenario': 'local-time-range'})
      boundary = -62135596800 + 86400 - (30472 if timezone == 'Asia/Seoul' else 0)
      for epoch in (boundary - 1, boundary): cases.append({'operation': 'date', 'epoch': epoch, 'TZ': timezone, 'scenario': 'previous-day-fold-probe-boundary'})
    if args.policy_only: cases = [case for case in cases if case['operation'] in ('catalog', 'date')]
    for case in cases:
      input_value = dict(defaults, **case); previous = os.environ.get('TZ'); os.environ['TZ'] = case.get('TZ', 'Asia/Seoul'); time.tzset(); env['TZ'] = os.environ['TZ']
      match case['operation']:
        case 'catalog': value = catalog.build_videos()
        case 'relative': value = paths.relative_time(case['epoch'])
        case 'file_id': value = catalog.file_id(case['path'])
        case 'token': value = hashlib.sha1(case['id'].encode('utf8', errors='ignore')).hexdigest()[:24]
        case 'date': value = catalog.date_label(case['epoch'])
        case _: raise RuntimeError('unhandled fixture operation')
      native = native_policy(binary, input_value, env); original = {'value': value}; row = {'surface': 'policy', 'input': input_value, 'original': original, 'native': native}; observations.append(row)
      if original != native: failures.append(row)
      if previous is None: os.environ.pop('TZ', None)
      else: os.environ['TZ'] = previous
      time.tzset()
  if args.policy_only:
    save(output / 'observations.json', observations); save(output / 'failures.json', failures); save(output / 'summary.json', {'policy_pairs': len(observations), 'failures': len(failures), 'native_exit_code': 0, 'limits': ['only affected catalog/date policy rerun; unchanged HTTP and FFmpeg proof retained']})
    if failures: raise SystemExit(1)
    return
  os.environ['TZ'] = env['TZ'] = 'Asia/Seoul'; time.tzset(); env['PATH'] = str(tools)
  previous_path = os.environ['PATH']; os.environ['PATH'] = str(tools)
  source_app = web.Application(); routes.register(source_app)
  async def set_clock(request: web.Request) -> web.Response:
    clock[0] = (await request.json())['now']; return web.json_response({'ok': True})
  source_app.router.add_post('/__fixture/clock', set_clock)
  runner = web.AppRunner(source_app); await runner.setup(); site = web.TCPSite(runner, '127.0.0.1', 0); await site.start(); source_port = site._server.sockets[0].getsockname()[1]
  state = output / 'owned-state'; (state / 'owned-web').mkdir(parents=True); (state / 'owned-assets').mkdir(); (state / 'settings.json').write_text('{"params": []}')
  native = await asyncio.create_subprocess_exec(str(binary), stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE, env=env)
  input_value = dict(defaults, http=True, composed=args.composed_only, state=str(state)); native.stdin.write((json.dumps(input_value) + '\n').encode()); await native.stdin.drain(); ready = await asyncio.wait_for(native.stdout.readline(), 10)
  if not ready: raise RuntimeError((await native.stderr.read()).decode())
  native_port = json.loads(ready)['port']; save(output / 'http-fixture.json', {'native_input': input_value, 'source_port': source_port, 'native_port': native_port})
  async with aiohttp.ClientSession(auto_decompress=False) as client:
    async def capture(port: int, method: str, path: str, headers: dict[str, str] | None = None, payload: dict[str, Any] | None = None) -> dict[str, Any]:
      async with client.request(method, f'http://127.0.0.1:{port}{path}', headers=headers, json=payload, allow_redirects=False) as response:
        body = await response.read(); return {'status': response.status, 'headers': {name: response.headers[name] for name in HEADERS if name in response.headers}, 'body_base64': base64.b64encode(body).decode()}
    async def pair(scenario: str, path: str, method: str = 'GET', headers: dict[str, str] | None = None) -> dict[str, Any]:
      original = await capture(source_port, method, path, headers); result = await capture(native_port, method, path, headers); row = {'surface': 'application' if args.composed_only else 'http', 'scenario': scenario, 'method': method, 'path': path, 'request_headers': headers or {}, 'original': original, 'native': result}; observations.append(row)
      if original != result: failures.append(row)
      return original
    async def now(value: float) -> None:
      for port in (source_port, native_port):
        result = await capture(port, 'POST', '/__fixture/clock', payload={'now': value})
        if result['status'] != 200: raise RuntimeError('fixture clock failed')
    rows = catalog.build_videos(); ids = {item['name']: item['id'] for item in rows}; normal = '/api/screenrecord/video/' + ids['normal.mp4']; synthetic = ids['synthetic.mp4']; thumb = '/api/screenrecord/thumbnail/' + synthetic
    if args.composed_only:
      await pair('application-list', '/api/screenrecord/videos')
      await pair('application-video-range', normal, headers={'Range': 'bytes=2-7'})
      await pair('application-download-head', '/api/screenrecord/download/' + ids['차량 🚗"\\.MP4'], 'HEAD')
      (tools / 'mode').write_text('stderr')
      await pair('application-provider-error', '/api/screenrecord/thumbnail/' + ids['normal.mp4'])
      (tools / 'mode').write_text('success')
      await pair('application-thumbnail-provider-recovery', thumb)
    else:
      await pair('initial-cache-before-three', '/api/screenrecord/videos'); await now(3.); await pair('catalog-list', '/api/screenrecord/videos'); await pair('catalog-head', '/api/screenrecord/videos', 'HEAD')
      for query in ('?offset=1&limit=2', '?offset=-1&limit=0', '?offset=&limit=', '?offset=9999999999999999999999999999&limit=9999999999999999999999999', '?offset=1_0&limit=%20%2B2%20', '?offset=bad', '?limit=1.2', '?offset=1&offset=2&limit=1'):
        await pair('pagination-boundary', '/api/screenrecord/videos' + query)
      for name in ('normal.mp4', 'clip.mkv', 'clip.avi', 'clip.mov', 'clip.ts', 'clip.hevc', '.mp4', '차량 🚗"\\.MP4'):
        await pair('mime-extension-disposition', '/api/screenrecord/video/' + ids[name] + '?download=0')
      baseline = await pair('video', normal); await pair('video-head', normal, 'HEAD'); await pair('download', '/api/screenrecord/download/' + ids['차량 🚗"\\.MP4']); await pair('download-empty-query', normal + '?download=')
      for headers in ({'Range': 'bytes=2-7'}, {'Range': 'bytes=-4'}, {'Range': 'bytes=9999-'}, {'If-None-Match': baseline['headers']['etag']}, {'If-Match': '"owned-mismatch"'}): await pair('shared-file-engine', normal, headers=headers)
      for value in ('unknown', 'x' * 65, 'a/b', 'a\\b', ' '): await pair('id-errors', '/api/screenrecord/video/' + quote(value, safe=''))
      await pair('method-rejected', normal, 'POST'); await pair('whitespace-id-stripped', '/api/screenrecord/video/' + quote(' ' + ids['normal.mp4'] + ' ', safe=''))
      fresh = first / 'fresh.mp4'; fresh.write_bytes(b'owned new'); os.utime(fresh, ns=(STAMP_NS + 5000000000, STAMP_NS + 5000000000)); await now(5.999); await pair('cached-list-stale', '/api/screenrecord/videos'); await pair('find-rebuild-before-cache-expiry', '/api/screenrecord/video/' + catalog.file_id(str(fresh))); await now(6.); await pair('cache-at-expiry', '/api/screenrecord/videos')
      fresh.unlink(); await pair('deleted-video-before-list-expiry', '/api/screenrecord/video/' + catalog.file_id(str(fresh))); await now(9.); await pair('cache-recovery-after-delete', '/api/screenrecord/videos')
      baseline = await pair('actual-ffmpeg-thumbnail', thumb); commands = (tools / 'commands.jsonl').read_text().splitlines(); await pair('positive-thumbnail-cache-no-provider', thumb)
      if len((tools / 'commands.jsonl').read_text().splitlines()) != len(commands): raise RuntimeError('positive cache reran FFmpeg')
      await pair('thumbnail-conditional', thumb, headers={'If-None-Match': baseline['headers']['etag']}); await pair('thumbnail-range', thumb, headers={'Range': 'bytes=0-9'}); await pair('thumbnail-raw-whitespace-cache-key', '/api/screenrecord/thumbnail/' + quote(' ' + synthetic + ' ', safe=''))
      for mode in ('stderr', 'stdout', 'no-output', 'empty', 'missing', 'success'):
        for cache in caches:
          path = cache / 'screen_thumb' / (hashlib.sha1(synthetic.encode()).hexdigest()[:24] + '.jpg'); path.unlink(missing_ok=True)
        (tools / 'mode').write_text(mode)
        if mode == 'missing': ffmpeg.rename(tools / 'disabled')
        await pair('thumbnail-provider-' + mode, thumb)
        if mode == 'missing': (tools / 'disabled').rename(ffmpeg)
      save(output / 'positive-cache-provider-count.json', {'commands_before_cached_request': len(commands), 'no_extra_commands': True})
    save(output / 'observations.json', observations); save(output / 'failures.json', failures)
  native.stdin.write(b'\n'); await native.stdin.drain(); stdout, stderr = await asyncio.wait_for(native.communicate(), 15); (output / 'native-stdout.txt').write_bytes(stdout); (output / 'native-stderr.txt').write_bytes(stderr); await runner.cleanup(); os.environ['PATH'] = previous_path
  commands = [json.loads(line) for line in (tools / 'commands.jsonl').read_text().splitlines()]; save(output / 'provider-commands.json', commands)
  provider_pairs = []; command_failures = []
  for index in range(0, len(commands), 2):
    original, result = commands[index:index + 2]; left, right = dict(original), dict(result); left['argv'] = [arg.replace(str(caches[0]), '$OWNED_CACHE') for arg in left['argv']]; right['argv'] = [arg.replace(str(caches[1]), '$OWNED_CACHE') for arg in right['argv']]; pair_row = {'original': original, 'native': result, 'equal_after_cache_root_normalization': left == right}; provider_pairs.append(pair_row)
    if left != right: command_failures.append(pair_row)
  save(output / 'provider-comparison.json', provider_pairs)
  for cache in caches: save(output / (cache.name + '-snapshot.json'), [{'path': str(path.relative_to(cache)), 'size': path.stat().st_size, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()} for path in cache.rglob('*') if path.is_file()])
  summary = {'policy_pairs': sum(row['surface'] == 'policy' for row in observations), 'http_pairs': sum(row['surface'] != 'policy' for row in observations), 'failures': len(failures), 'provider_pairs': len(provider_pairs), 'provider_failures': len(command_failures), 'native_exit_code': native.returncode, 'limits': ['owned synthetic files and loopback only', 'external FFmpeg codec availability and runtime device paths remain provider/target gates', 'shared file protocol corpus reused; only family-specific representative Range/conditional cases repeated']}; save(output / 'summary.json', summary)
  if failures or command_failures or native.returncode: raise SystemExit(1)

if __name__ == '__main__': asyncio.run(main())
