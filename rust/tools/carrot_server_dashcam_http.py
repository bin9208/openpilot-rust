#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import asyncio
from pathlib import Path
import sys

from carrot_server_dashcam_http.fixture import Fixture, prepare, save
from carrot_server_dashcam_http.keepalive import run as keepalive
from carrot_server_dashcam_http.scenarios import cache, empty_cache, pages, populated, read_state

async def main() -> None:
  parser = argparse.ArgumentParser(); parser.add_argument('--binary', type=Path, required=True); parser.add_argument('--output', type=Path, required=True); parser.add_argument('--composed-only', action='store_true'); parser.add_argument('--keepalive-only', action='store_true'); args = parser.parse_args()
  output = args.output.resolve(); binary = args.binary.resolve(); prepare(binary, output, sys.argv)
  observations = []; failures = []; native_exits = []
  suites = ('keepalive',) if args.keepalive_only else ('composed',) if args.composed_only else ('empty-cache', 'catalogue-and-state')
  for suite in suites:
    directory = output / suite; directory.mkdir()
    fixture = Fixture(binary, directory, args.composed_only)
    if suite in ('composed', 'catalogue-and-state'): populated(fixture)
    await fixture.start()
    try:
      if suite == 'keepalive': await keepalive(fixture)
      elif suite == 'empty-cache': await empty_cache(fixture)
      elif suite == 'composed':
        await fixture.pair('application-catalogue', '/api/dashcam/routes?limit=1&segment_limit=2')
        await fixture.pair('application-encoded-prefix', '/api/%64ashcam/routes', 'HEAD')
        await fixture.pair('application-segment-page', '/api/dashcam/segments/0000000b--bbbbbbbbbb?offset=1&limit=1&sort=desc')
        await fixture.pair('application-recent', '/api/dashcam/recent?limit=2')
        await fixture.pair('application-read-state-post', '/api/dashcam/read-state', 'POST', b'{"recentSegment":"owned--1"}', {'Content-Type': 'application/json'}, files=True)
        await fixture.pair('application-read-state-head', '/api/dashcam/read-state', 'HEAD', files=True)
      else: await pages(fixture); await cache(fixture); await read_state(fixture)
    finally:
      await fixture.close()
      save(directory / 'observations.json', fixture.observations); save(directory / 'failures.json', fixture.failures)
    observations.extend(fixture.observations); failures.extend(fixture.failures); native_exits.append(fixture.native.returncode)
  save(output / 'result.json', {'pairs': len(observations), 'failures': len(failures), 'native_exit_codes': native_exits,
    'surface': 'actual source aiohttp and native TCP listeners, owned synthetic filesystem/state only',
    'observable': 'raw status, content-type/content-length/Allow and body bytes; exact state and temp bytes after POST; clock/bounds fixture controls',
    'limits': ['only selected original source catalogue/read-state functions; no report/replay/media/upload execution', 'existing JSON/request-text/protocol and unchanged catalogue/path/read-state prerequisite proof reused', 'runtime/device/NAS acceptance remains open']})
  if failures or any(native_exits): raise SystemExit(1)

if __name__ == '__main__': asyncio.run(main())
