"""Reproduce the inherited UAS result-window accumulation at its transfer seam."""

from __future__ import annotations
import argparse
import ast
import contextlib
import functools
import json
from pathlib import Path
import types

ROOT = Path(__file__).resolve().parents[2]


def run(count: int) -> dict:
  path = ROOT / 'tinygrad_repo/tinygrad/runtime/support/usb.py'
  tree = ast.parse(path.read_text())
  tree.body = [ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0)] + [
    node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'USB3'
  ]
  ast.fix_missing_locations(tree)
  env = {'functools': functools, 'usbgpu_bus_lock': contextlib.nullcontext}
  exec(compile(tree, str(path), 'exec'), env)
  usb = env['USB3'].__new__(env['USB3'])
  usb.use_bot, usb.max_streams = False, 31
  usb.ep_cmd_out, usb.ep_stat_in, usb.ep_data_in, usb.ep_data_out = 4, 0x83, 0x81, 2
  usb.buf_cmd = [bytearray(32) for _ in range(31)]
  usb.buf_stat = [bytearray(64) for _ in range(31)]
  usb.buf_data_in = [bytearray(4096) for _ in range(31)]
  usb.buf_data_out = [bytearray(32) for _ in range(31)]
  usb.buf_data_out_mvs = [memoryview(value) for value in usb.buf_data_out]
  usb.tr = {ep: list(range(31)) for ep in [4, 0x83, 0x81, 2]}
  windows = []
  usb._prep_transfer = lambda slot, ep, stream, buf, length: types.SimpleNamespace(ep=ep, buf=buf, length=length)
  cursor = 0

  def submit(transfers):
    nonlocal cursor
    reads = [transfer for transfer in transfers if transfer.ep == 0x81]
    windows.append(len(reads))
    for transfer in reads:
      transfer.buf[0] = cursor
      cursor += 1

  usb._submit_and_wait = submit
  results = usb.send_batch([b'\xe4\x01\x50\x00\x00\x00'] * count, [1] * count)
  return {'command_count': count, 'transfer_windows': windows, 'result_count': len(results), 'result_first_bytes': [value[0] for value in results]}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--evidence', required=True, type=Path)
  args = parser.parse_args()
  results = [run(count) for count in [1, 30, 31, 32, 33, 62]]
  assert results[2]['result_count'] == 31
  assert results[3]['result_count'] == 63
  assert results[4]['result_first_bytes'][32] == 1
  args.evidence.write_text(json.dumps({'source': 'tinygrad_repo/tinygrad/runtime/support/usb.py', 'results': results}, indent=2) + '\n')
  print('REPRODUCED: 32 commands return 63 results; the 33rd result in a 33-command batch is stale slot 1')


if __name__ == '__main__':
  main()
