from __future__ import annotations

import argparse
import asyncio
import json
import os
from pathlib import Path
import shutil
import sys

import aiohttp

from carrot_navi_qa.process import Process, available_port, digest
from carrot_navi_qa.socket import Client

CHECKOUT_ROOT = Path(__file__).resolve().parents[2]


async def run(args: argparse.Namespace) -> None:
  root = CHECKOUT_ROOT
  port = available_port()
  common = ['--host', '127.0.0.1', '--port', str(port), '--no-beacon']
  if not args.cereal:
    common.append('--no-cereal')
  params_root = args.output / 'params'
  prefix = 'cn206-' + args.output.name
  queues = Path('/dev/shm') / ('msgq_' + prefix)
  if args.cereal:
    queues.mkdir(exist_ok=False)
  if args.cereal:
    for path in args.pythonpath:
      sys.path.insert(0, path)
    os.environ['OPENPILOT_PREFIX'] = prefix
  environment = dict(os.environ, PARAMS_ROOT=str(params_root), OPENPILOT_PREFIX=prefix,
    PYTHONPATH=os.pathsep.join([*args.pythonpath, str(root / 'rust/tools'), str(root)]))
  command = [str(args.native_bin), *common] if args.native_bin else [str(args.source_python), '-P',
    str(root / 'rust/tools/carrot_navi_process_source.py'), '--binding', str(args.binding),
    '--params-root', str(params_root), *common]
  process = Process(command, root, args.output, environment)
  process.start()
  observer = None
  client = None
  try:
    async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=5)) as http:
      url = f'http://127.0.0.1:{port}'
      await process.ready(http, url)
      if args.cereal:
        from carrot_navi_qa.cereal import Observer
        observer = Observer(args.output)
      client = Client(http, url, observer=observer.observe if observer else None,
        register_media=observer.register_media if observer else None)
      if args.lifecycle:
        from carrot_navi_qa.lifecycle import boundaries
        await boundaries(client, observer, params_root, args.binding)
      else:
        await client.normal()
      args.output.joinpath('rows.json').write_text(json.dumps(client.rows, ensure_ascii=True, indent=2))
      if observer:
        observer.save()
    status = await process.stop()
    args.output.joinpath('result.json').write_text(json.dumps({'status': 'pass', 'returncode': status,
      'actions': len(client.rows), 'source_sha256': digest(root / 'openpilot/selfdrive/carrot/carrot_navi.py'),
      'publisher_sha256': digest(root / 'openpilot/selfdrive/carrot/carrot_navi_cereal.py')}, indent=2))
    if status != 0:
      raise RuntimeError(f'signal termination returned {status}')
  except BaseException as error:
    if client:
      args.output.joinpath('rows.json').write_text(json.dumps(client.rows, ensure_ascii=True, indent=2))
    if observer:
      observer.save()
    args.output.joinpath('failure.json').write_text(json.dumps({'kind': type(error).__name__, 'message': str(error)}, indent=2))
    if process.process.poll() is None:
      await process.stop()
    raise
  finally:
    if args.cereal:
      shutil.rmtree(queues)


def main() -> None:
  parser = argparse.ArgumentParser()
  runtime = parser.add_mutually_exclusive_group(required=True)
  runtime.add_argument('--native-bin', type=Path)
  runtime.add_argument('--source-python', type=Path)
  parser.add_argument('--binding', type=Path)
  parser.add_argument('--pythonpath', action='append', default=[])
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--cereal', action='store_true')
  parser.add_argument('--lifecycle', action='store_true')
  args = parser.parse_args()
  if args.source_python and not args.binding:
    parser.error('source mode requires actual Params binding')
  if args.lifecycle and (not args.cereal or not args.binding):
    parser.error('lifecycle mode requires Cereal and actual Params binding')
  asyncio.run(run(args))


if __name__ == '__main__':
  main()
