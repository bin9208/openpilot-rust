#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def main():
  parser = argparse.ArgumentParser(description='Run the complete owned Athena source/runtime comparison without production services or hardware')
  parser.add_argument('--bin-dir', required=True, type=Path)
  parser.add_argument('--output', required=True, type=Path)
  args = parser.parse_args()
  binary = args.bin_dir.resolve()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  environment = dict(os.environ, PYTHONPATH=str(ROOT) + ':' + str(ROOT / 'rust/tools'))
  vision = output / 'athena-vision-peer'
  cases = [
    ('vision-build', ['build_athena_vision_peer.py', str(vision)]),
    ('policy', ['check_athena_policy.py', str(binary / 'examples/athena_policy'), str(output / 'policy.json')]),
    ('rpc', ['check_athena_rpc.py', str(binary / 'examples/athena_rpc'), str(output / 'rpc.json')]),
    ('ipc', ['check_athena_ipc.py', str(binary / 'examples/athena_ipc'), str(output / 'ipc.json')]),
    ('image', ['check_athena_image.py', str(binary / 'examples/athena_image'), str(output / 'image')]),
    ('daemon', ['check_athena_daemon.py', str(binary / 'openpilot-athenad'), str(binary / 'examples/athena_ipc'), str(output / 'daemon')]),
    ('interrupted-connect', ['check_athena_connect.py', '--binary', str(binary / 'examples/athena_upload'),
                             '--output', str(output / 'interrupted-connect')]),
    ('transfers', ['check_athena_transfers.py', str(binary / 'openpilot-athenad'), str(binary / 'examples/athena_ipc'), str(output / 'transfers')]),
    ('upload-edges', ['check_athena_upload_edges.py', str(binary / 'openpilot-athenad'), str(output / 'upload-edges')]),
    ('metered-abort', ['check_athena_metered_abort.py', str(binary / 'openpilot-athenad'), str(binary / 'examples/athena_ipc'), str(output / 'metered-abort')]),
    ('forwarding', ['check_athena_forwarding.py', str(binary / 'examples/athena_forwarding'), str(output / 'forwarding')]),
    ('snapshot', ['check_athena_snapshot.py', str(binary / 'openpilot-athenad'), str(binary / 'examples/athena_ipc'), str(vision), str(output / 'snapshot')]),
    ('camera-lifecycle', ['check_athena_camera_lifecycle.py', str(binary / 'examples/athena_snapshot'), str(binary / 'openpilot-process-child'),
                          str(binary / 'examples/athena_ipc'), str(vision), str(output / 'camera-lifecycle')]),
    ('supervisor', ['check_athena_supervisor.py', str(binary / 'openpilot-manage-athenad'), str(binary / 'examples/athena_log_capture'),
                   str(output / 'supervisor')]),
    ('proxy', ['check_athena_proxy.py', str(binary / 'openpilot-athenad'), str(output / 'proxy')]),
    ('proxy-backpressure', ['check_athena_proxy_backpressure.py', str(binary / 'openpilot-athenad'), str(output / 'proxy-backpressure')]),
    ('reconnect', ['check_athena_reconnect.py', str(binary / 'openpilot-athenad'), str(output / 'reconnect')]),
  ]
  receipts = []
  for name, command in cases:
    command = [sys.executable, str(ROOT / 'rust/tools' / command[0]), *command[1:]]
    if name in ['proxy', 'proxy-backpressure', 'interrupted-connect']:
      namespace = ['unshare', '--net'] if os.geteuid() == 0 else ['unshare', '--user', '--map-root-user', '--net']
      command = [*namespace, 'sh', '-c', 'ip link set lo up && exec "$@"', 'sh', *command]
    with (output / f'{name}.log').open('w') as log:
      process = subprocess.run(command, env=environment, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=180)
    receipts.append({'scenario': name, 'command': command, 'returncode': process.returncode, 'log': str(output / f'{name}.log')})
    (output / 'suite.json').write_text(json.dumps(receipts, indent=2) + '\n')
    assert process.returncode == 0, receipts[-1]
    print(f'PASS {name}', flush=True)


if __name__ == '__main__':
  main()
