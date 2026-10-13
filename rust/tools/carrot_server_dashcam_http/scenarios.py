from __future__ import annotations

import json
import os
from urllib.parse import quote

from .fixture import Fixture

A = '0000000a--aaaaaaaaaa'
B = '0000000b--bbbbbbbbbb'
C = '0000000c--cccccccccc'
LEGACY = '2026-10-08--차량'

def populated(fixture: Fixture) -> None:
  for index, epoch in ((0, 100.), (1, 165.), (2, 200.), (4, 290.)):
    fixture.marker(f'{A}--{index}', epoch, index != 2)
  (fixture.root / f'{A}--2' / 'qcamera.ts').write_bytes(b'')
  for index, epoch in ((0, 400.), (1, 450.), (2, 470.), (3, 530.)):
    fixture.marker(f'{B}--{index}', epoch, index != 1)
  (fixture.root / f'{B}--3' / 'rlog.lock').write_bytes(b'owned lock')
  fixture.marker(f'{C}--0', 600., False)
  fixture.marker(f'{LEGACY}--0', 80.)

async def pages(fixture: Fixture) -> None:
  route = '/api/dashcam/routes'; segments = '/api/dashcam/segments/' + B
  cases = (
    ('newest-tail-hidden-only-older-incomplete-visible', route),
    ('route-list-offset-page', route + '?offset=1&limit=1&segment_limit=2'),
    ('route-desc-display-window', route + '?limit=1&segment_limit=2&sort=desc'),
    ('ascending-segment-page-preceding-seed', segments + '?offset=1&limit=1'),
    ('descending-segment-page-preceding-seed', segments + '?offset=1&limit=1&sort=desc'),
    ('segment-offset-clamps-to-total', segments + '?offset=1000000'),
    ('route-offset-echo-beyond-total', route + '?offset=1000001'),
    ('huge-int-clamps-without-width-fallback', route + '?limit=' + '9' * 50 + '&segment_limit=' + '9' * 50),
    ('negative-query-lower-bound', segments + '?offset=-9&limit=-9'),
    ('huge-negative-offset-clamps-to-zero', segments + '?offset=-' + '9' * 50),
    ('invalid-int-fallback-and-sort', route + '?limit=bad&segment_limit=bad&offset=bad&sort=anything'),
    ('first-duplicate-query-value', route + '?limit=1&limit=2'),
    ('unicode-int-and-python-sort-whitespace', segments + '?limit=%EF%BC%91&sort=%1CDESC%1F'),
    ('legacy-unicode-route', '/api/dashcam/segments/' + quote(LEGACY, safe='')),
    ('encoded-static-prefix', '/api/%64ashcam/%72outes'),
    ('encoded-dynamic-slash-remains-invalid', '/api/dashcam/segments/' + B + '%2Fowned'),
    ('missing-route', '/api/dashcam/segments/missing'),
    ('empty-query-default', route + '?offset=&limit=&segment_limit='),
  )
  for scenario, path in cases: await fixture.pair(scenario, path)
  await fixture.pair('routes-head-representation-length', route, 'HEAD')
  await fixture.pair('segments-head-representation-length', segments, 'HEAD')
  for limit in (2, 5, 10): await fixture.pair('recent-complete-only-' + str(limit), '/api/dashcam/recent?limit=' + str(limit))
  for scenario, value in (('missing', ''), ('unsupported', '3'), ('invalid-int', 'bad'), ('isdigit-int-mismatch', '²')):
    await fixture.pair('recent-limit-' + scenario, '/api/dashcam/recent?limit=' + quote(value, safe=''))
  await fixture.pair('recent-head', '/api/dashcam/recent?limit=2', 'HEAD')
  await fixture.pair('routes-method-allow', route, 'POST')
  await fixture.pair('read-state-method-allow', '/api/dashcam/read-state', 'PUT')

async def cache(fixture: Fixture) -> None:
  route = '/api/dashcam/routes'; child = fixture.root / f'{B}--3'; signature = (fixture.root.stat().st_mtime_ns, fixture.root.stat().st_size)
  child.joinpath('rlog.lock').unlink()
  await fixture.pair('child-lock-completion-visible-without-name-rebuild', route + '?limit=1&segment_limit=4')
  if signature != (fixture.root.stat().st_mtime_ns, fixture.root.stat().st_size): raise RuntimeError('child mutation unexpectedly changed root signature')
  fixture.marker(f'{C}--0', 600., True)
  await fixture.pair('hidden-route-completes-without-root-mutation', route + '?limit=1')
  os.utime(fixture.root / f'{B}--0' / 'qcamera.ts', (800., 800.))
  await fixture.pair('positive-end-cache-retains-child-time', '/api/dashcam/segments/' + B + '?limit=1')
  fixture.marker(f'{A}--2', 230., True)
  await fixture.pair('previous-zero-end-is-not-cached', '/api/dashcam/segments/' + A)
  await fixture.control('fresh-cache-just-before-max-age', operation='clock', now=300.999)
  await fixture.pair('positive-cache-fresh-before-300', '/api/dashcam/segments/' + B + '?limit=1')
  await fixture.control('exact-300-max-age', operation='clock', now=301.)
  await fixture.pair('expiry-invalidates-positive-end-before-hydration', '/api/dashcam/segments/' + B + '?limit=1')
  os.utime(fixture.root / f'{B}--0' / 'qcamera.ts', (900., 900.))
  fixture.marker('0000000d--dddddddddd--0', 1000., True)
  await fixture.pair('root-name-mutation-immediate-rebuild', route)
  await fixture.pair('root-rebuild-clears-positive-end-cache', '/api/dashcam/segments/' + B + '?limit=1')
  cached_signature = (fixture.root.stat().st_mtime_ns, fixture.root.stat().st_size)
  await fixture.control('failure-expired-clock', operation='clock', now=601.)
  fixture.root.chmod(0)
  try:
    await fixture.pair('expired-rebuild-filesystem-error', route)
    await fixture.pair('invalid-route-still-builds-before-validation', '/api/dashcam/segments/' + B + '%2Fowned')
    await fixture.pair('recent-filesystem-error', '/api/dashcam/recent?limit=2')
  finally: fixture.root.chmod(0o700)
  await fixture.control('rewind-after-failed-rebuild', operation='clock', now=302.)
  hidden = fixture.root / '0000000e--eeeeeeeeee--0'; fixture.marker(hidden.name, 1100., True)
  os.utime(fixture.root, ns=(cached_signature[0], cached_signature[0]))
  if cached_signature != (fixture.root.stat().st_mtime_ns, fixture.root.stat().st_size): raise RuntimeError('owned fixture could not preserve cached root signature')
  await fixture.pair('failed-rebuild-retains-old-name-cache', route)
  await fixture.control('recovery-time', operation='clock', now=602.)
  await fixture.pair('error-recovery-normal-page', route)

async def empty_cache(fixture: Fixture) -> None:
  route = '/api/dashcam/routes'
  await fixture.pair('first-none-cache-builds-empty', route)
  probe = fixture.marker('unindexed', 100., False)
  await fixture.pair('unindexed-root-mutation-rebuilds-empty', route)
  await fixture.control('seed-positive-end-cache-under-empty-index', operation='bounds', segments=['unindexed'])
  os.utime(probe / 'qcamera.ts', (500., 500.))
  await fixture.pair('cached-empty-is-not-none', route)
  await fixture.control('empty-index-hit-preserves-positive-time', operation='bounds', segments=['unindexed'])
  await fixture.control('empty-index-max-age', operation='clock', now=301.)
  await fixture.pair('empty-index-expiry', route)
  await fixture.control('expired-empty-index-clears-positive-time', operation='bounds', segments=['unindexed'])

async def read_state(fixture: Fixture) -> None:
  path = '/api/dashcam/read-state'; headers = {'Content-Type': 'application/json; charset=utf-8'}
  await fixture.pair('missing-read-state', path, files=True)
  for scenario, raw in (('list', b'[]'), ('malformed', b'{broken'), ('nonscalar-field', b'{"recentSegment":["owned--1"]}')):
    for state in fixture.states: state.parent.mkdir(exist_ok=True); state.write_bytes(raw)
    await fixture.pair('existing-state-' + scenario, path, files=True)
  await fixture.pair('read-state-head', path, 'HEAD')
  for scenario, value in (('null', None), ('list', []), ('empty-object', {}), ('invalid-name', {'recentSegment': 'invalid'}),
      ('unicode-normalization', {'recentSegment': ' 2026-10-08--차량--１ '}),
      ('surrogate-error', {'recentSegment': '\ud800--1'}), ('surrogate-error-recovery', {'recentSegment': 'owned--2'})):
    await fixture.pair('post-state-' + scenario, path, 'POST', json.dumps(value, ensure_ascii=True).encode(), headers, files=True)
  await fixture.pair('malformed-post-falls-back-to-empty-object', path, 'POST', b'{broken', headers, files=True)
  body = json.dumps({'recentSegment': 'owned--３'}, ensure_ascii=False).encode('utf-8')
  await fixture.pair('post-state-real-unicode-utf8', path, 'POST', body, headers, files=True)
  await fixture.pair('read-written-state', path, files=True)
  for state in fixture.states: state.unlink(); state.mkdir()
  await fixture.pair('post-replace-destination-directory-is-bare500', path, 'POST', b'{"recentSegment":"owned--4"}', headers, files=True)
  for state in fixture.states: state.rmdir()
  await fixture.pair('post-replace-error-recovery', path, 'POST', b'{"recentSegment":"owned--5"}', headers, files=True)
  await fixture.pair('read-state-get-after-recovery', path, files=True)
