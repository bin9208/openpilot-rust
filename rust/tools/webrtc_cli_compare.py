# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "av==16.1.0", "aiohttp==3.13.3", "dnspython==2.8.0"]
# ///
# Run with the pinned source-oracle environment and owned output/IPC roots.
"""Observe normal original/native standalone CLI, HTTP and owned Params."""

import asyncio
import json
import os
from pathlib import Path
import signal
import socket
import sys
import tempfile

from aiohttp import ClientSession, ClientConnectorError
from webrtc_test_peer import save, until


async def scenario(mode, carrot, output, target):
  folder = output / f'{mode}-{carrot}'
  folder.mkdir()
  with socket.socket() as reserved:
    reserved.bind(('127.0.0.1', 0))
    port = reserved.getsockname()[1]
  root = folder / 'params'
  affinity = sorted(os.sched_getaffinity(0))
  environment = dict(
    os.environ, WEBRTC_OWNED_ROOT=str(folder), PARAMS_ROOT=str(root), CARROT_VISION_WEBRTC_CORES=os.environ.get('WEBRTC_TEST_CORES', str(affinity[0]))
  )
  assert root.parent == folder and environment['OPENPILOT_PREFIX']
  options = ['--host', '127.0.0.1', '--port', str(port)]
  argv = (
    [sys.executable, '-P', str(Path(__file__).with_name('webrtc_cli_source.py')), carrot, *options]
    if mode == 'source'
    else [str(target / ('openpilot-carrot-webrtcd' if carrot == 'carrot' else 'openpilot-webrtcd')), *options]
  )
  row = {
    'mode': mode,
    'profile': carrot,
    'argv': argv,
    'params_root': str(root),
    'inherited_affinity': affinity,
    'configured_cores': environment['CARROT_VISION_WEBRTC_CORES'],
  }
  process = await asyncio.create_subprocess_exec(*argv, env=environment, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
  row['pid'] = process.pid
  try:
    async with ClientSession() as http:
      async with asyncio.timeout(4):
        while True:
          try:
            async with http.get(f'http://127.0.0.1:{port}/schema?services=carState') as response:
              row['schema_status'], row['schema'] = response.status, await response.json()
            break
          except ClientConnectorError:
            if process.returncode is not None:
              raise
            await asyncio.sleep(0.01)
      row['running_affinity'] = sorted(os.sched_getaffinity(process.pid))
      if carrot == 'carrot':
        key = root / environment['OPENPILOT_PREFIX'] / 'CarrotVisionActive'
        await until(key.exists)
        row['params_initial'], row['params_key'] = key.read_bytes().hex(), str(key)
    process.send_signal(signal.SIGTERM)
    stdout, stderr = await asyncio.wait_for(process.communicate(), 4)
    row.update({'returncode': process.returncode, 'stdout': stdout.decode(), 'stderr': stderr.decode()})
    if carrot == 'carrot':
      row['params_stopped'] = key.read_bytes().hex()
    assert row['returncode'] == 0 and row['schema_status'] == 200
  finally:
    if process.returncode is None:
      process.kill()
      stdout, stderr = await process.communicate()
      row.update({'returncode': process.returncode, 'stdout': stdout.decode(), 'stderr': stderr.decode()})
    save(folder / 'result.json', row)
  return row


async def main(target, output):
  output.mkdir()
  with tempfile.TemporaryDirectory(prefix='msgq_rtc240_cli_', dir='/dev/shm') as namespace:
    os.environ['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
    rows = []
    for label, cores in [
      ('valid', str(min(os.sched_getaffinity(0)))),
      ('nonnumeric', 'bad'),
      ('negative', '-1'),
      ('large', '99999999999999999999999999999999'),
      ('unicode-underscores', '+٠_١'),
    ]:
      folder = output / label
      folder.mkdir()
      os.environ['WEBRTC_TEST_CORES'] = cores
      pair = [await scenario(mode, 'carrot', folder, target) for mode in ('source', 'native')]
      for row in pair:
        assert row['running_affinity'] == row['inherited_affinity'], row
        assert row['params_initial'] == row['params_stopped'] == '30', row
      assert ('failed setting Carrot Vision core affinity' in pair[0]['stderr']) == (label == 'nonnumeric'), pair[0]['stderr']
      assert ('WebRTC Carrot Vision affinity failed:' in pair[1]['stderr']) == (label == 'nonnumeric'), pair[1]['stderr']
      rows.extend(pair)
    save(output / 'result.json', rows)
    print(
      json.dumps(
        [
          {k: row.get(k) for k in ['mode', 'profile', 'running_affinity', 'inherited_affinity', 'params_initial', 'params_stopped', 'returncode']}
          for row in rows
        ],
        indent=2,
      )
    )


if __name__ == '__main__':
  asyncio.run(main(*(Path(value) for value in sys.argv[1:3])))
