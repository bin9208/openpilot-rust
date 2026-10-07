from __future__ import annotations

import argparse
import asyncio
import json
import os
from pathlib import Path
import signal
import socket
import time

import aiohttp

from carrot_navi_qa.process import Process

CHECKOUT_ROOT = Path(__file__).resolve().parents[3]


async def run(args: argparse.Namespace) -> None:
  root = CHECKOUT_ROOT
  prefix = 'cn206-' + args.output.name
  environment = dict(os.environ, PARAMS_ROOT=str(args.output / 'params'), OPENPILOT_PREFIX=prefix,
    PYTHONPATH=os.pathsep.join([os.environ.get('PYTHONPATH', ''), str(root / 'rust/tools'), str(root)]))
  with socket.socket() as occupied:
    occupied.bind(('127.0.0.1', 0))
    occupied.listen()
    port = occupied.getsockname()[1]
    common = ['--host', '127.0.0.1', '--port', str(port), '--no-cereal']
    if args.exhaust:
      common += ['--no-beacon']
    else:
      from openpilot.selfdrive.carrot.carrot_navi import _interface_ipv4_addresses
      interfaces = _interface_ipv4_addresses()
      assert interfaces == (('10.206.0.1', '255.255.255.0', None),), interfaces
      common += ['--advertise-ip', '10.206.0.1']
    command = [str(args.native_bin), *common] if args.native_bin else [str(args.source_python), '-P',
      str(root / 'rust/tools/carrot_navi_process_source.py'), '--binding', str(args.binding),
      '--params-root', str(args.output / 'params'), *common]
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as udp:
      if not args.exhaust:
        udp.bind(('0.0.0.0', 7705))
        udp.setblocking(False)
      process = Process(command, root, args.output, environment)
      process.start()
      try:
        deadline = time.monotonic() + 3
        while 'retrying in 0.5s' not in args.output.joinpath('stdout.log').read_text():
          if process.process.poll() is not None or time.monotonic() > deadline:
            raise TimeoutError('owned port did not trigger bind retry')
          await asyncio.sleep(.005)
        process.capture()
        if args.exhaust:
          deadline = time.monotonic() + 7
          while process.process.poll() is None:
            if time.monotonic() > deadline:
              raise TimeoutError('bind retry count was not bounded')
            await asyncio.sleep(.01)
          status = await process.stop()
          retries = args.output.joinpath('stdout.log').read_text().count('retrying in 0.5s')
          assert status == 1 and retries == 10, (status, retries)
          result = {'status': 'pass', 'returncode': status, 'bounded_bind_retries': retries}
        else:
          occupied.close()
          async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=3)) as http:
            await process.ready(http, f'http://127.0.0.1:{port}')
          while True:
            try:
              udp.recvfrom(1024)
            except BlockingIOError:
              break
          packets = []
          for _ in range(3):
            packet, source = await asyncio.wait_for(asyncio.get_running_loop().sock_recvfrom(udp, 1024), 2)
            packets.append({'wire_hex': packet.hex(), 'body': json.loads(packet),
              'source_ip': source[0], 'received_ns': time.monotonic_ns()})
          assert all(packet['body'] == {'ip': '10.206.0.1', 'navi_debug': 1} and
            packet['source_ip'] == '10.206.0.1' for packet in packets)
          gaps = [(right['received_ns'] - left['received_ns']) / 1e9 for left, right in zip(packets, packets[1:], strict=False)]
          assert all(.95 <= gap <= 1.2 for gap in gaps), gaps
          status = await process.stop(signal.SIGTERM)
          assert status == 0
          with socket.socket() as released:
            released.bind(('127.0.0.1', port))
          args.output.joinpath('discovery.json').write_text(json.dumps(packets, indent=2))
          result = {'status': 'pass', 'returncode': status, 'retry_recovered': True,
            'discovery_packets': 3, 'one_second_beacons': True, 'port_released': True}
        args.output.joinpath('result.json').write_text(json.dumps(result, indent=2))
      finally:
        if process.process.poll() is None:
          await process.stop()


def main() -> None:
  parser = argparse.ArgumentParser()
  implementation = parser.add_mutually_exclusive_group(required=True)
  implementation.add_argument('--native-bin', type=Path)
  implementation.add_argument('--source-python', type=Path)
  parser.add_argument('--binding', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--exhaust', action='store_true')
  asyncio.run(run(parser.parse_args()))


if __name__ == '__main__':
  main()
