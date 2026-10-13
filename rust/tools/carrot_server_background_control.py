#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import sys
import time
import uuid

import anyio
from carrot_server_dashcam_sync_probe import Peer, startup
from carrot_server_dashcam_upload import save


async def run(args: argparse.Namespace) -> None:
  output = args.output.resolve()
  output.mkdir(parents=True)
  namespace = 'rust-probe-background-' + uuid.uuid4().hex
  queue = Path('/dev/shm') / ('msgq_' + namespace)
  queue.mkdir()
  params = output / 'params' / namespace
  params.mkdir(parents=True)
  peers = [Peer(output / name) for name in ('source', 'native')]
  results = []
  try:
    for peer in peers:
      peer.output.mkdir()
      programs = peer.output / 'bin'
      programs.mkdir()
      program = programs / 'nmcli'
      program.write_text(f'#!{sys.executable}\nimport json,os,sys,time\nfrom pathlib import Path\nPath({str(peer.output / "network-children.jsonl")!r}).open("a").write(json.dumps({{"pid":os.getpid(),"args":sys.argv,"start":time.monotonic()}})+"\\n")\ntime.sleep(30)\n')
      program.chmod(0o755)
      env = {**os.environ, 'OPENPILOT_PREFIX': namespace, 'PARAMS_ROOT': str(output / 'params'), 'CARROT_DATA_DIR': str(output / 'data'), 'PATH': str(programs) + ':' + os.environ['PATH']}
      command = [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_live_source.py'))] if peer is peers[0] else [str(args.binary.resolve())]
      config = {'params': True, 'unavailable': False, 'composed': True, 'output': str(peer.output), 'state': str(output / 'state'), 'network_program': str(program)}
      await startup(peer, peer.start(command, config, env, True))
      with anyio.fail_after(2):
        while not (peer.output / 'network-children.jsonl').exists():
          await anyio.sleep(0.01)
      children = [json.loads(line) for line in (peer.output / 'network-children.jsonl').read_text().splitlines()]
      began = time.monotonic()
      await peer.stop()
      with anyio.fail_after(2):
        phase = json.loads(await peer.reader.receive_until(b'\n', 65536))
        cleaned = json.loads(await peer.reader.receive_until(b'\n', 65536))
      assert phase['stopping'] and cleaned == {'serve_returned': True}
      alive_at_return = all(Path(f'/proc/{child["pid"]}').exists() for child in children)
      assert alive_at_return and peer.process.returncode is None
      with anyio.fail_after(7):
        await peer.process.wait()
      elapsed = time.monotonic() - began
      children = [json.loads(line) for line in (peer.output / 'network-children.jsonl').read_text().splitlines()]
      assert len(children) == 2 and all(not Path(f'/proc/{child["pid"]}').exists() for child in children)
      assert peer.process.returncode == 0 and 4.5 < elapsed < 6.5
      results.append({'provider': peer.output.name, 'serve_return': cleaned, 'worker_child_alive_after_serve': alive_at_return, 'process_exit_seconds': elapsed, 'network_children': children, 'children_reaped_at_exit': True})
      save(output / 'observations.json', results)
  finally:
    with anyio.CancelScope(shield=True):
      for peer in peers:
        await peer.close()
      shutil.rmtree(queue)
  save(output / 'result.json', {'source_native_detached_worker_observed': True, 'actual_process_exit_pair': True, 'network_children_reaped': True})
  print('PASS')


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  await run(parser.parse_args())


if __name__ == '__main__':
  anyio.run(main)
