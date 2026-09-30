# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp==2.1.0", "pyzmq==27.2.0", "requests==2.32.5", "pyserial==3.5", "zstandard==0.25.0", "numpy==2.5.3"]
# ///
# uv run --no-project --python 3.12 rust/tools/check_hardware_info.py BINARY PARAMS_BINDING OUTPUT [--runner QEMU -L SYSROOT]
"""Compare native hardware APIs with unchanged source on isolated real surfaces."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import tempfile
import threading
import time

from hardware_info_cases import build_cases

SHIMS = {
  'sudo': '''#!/bin/sh
printf '%s\\0' sudo "$@" >> "$HW_INFO_COMMAND_LOG"
printf '\\0' >> "$HW_INFO_COMMAND_LOG"
[ "$1" = cat ] || exit 91
shift
for item do
  case "$item" in "$HW_INFO_FIXTURE_ROOT"/*) ;; *) exit 92 ;; esac
done
exec /bin/cat "$@"
''',
  'iwlist': '''#!/bin/sh
printf '%s\\0' iwlist "$@" >> "$HW_INFO_COMMAND_LOG"
printf '\\0' >> "$HW_INFO_COMMAND_LOG"
[ "$1" = wlan0 ] && [ "$2" = scan ] || exit 93
if [ -f "$HW_INFO_FIXTURE_ROOT/commands/iwlist.stdout" ]; then /bin/cat "$HW_INFO_FIXTURE_ROOT/commands/iwlist.stdout"; fi
code=0
if [ -f "$HW_INFO_FIXTURE_ROOT/commands/iwlist.status" ]; then IFS= read -r code < "$HW_INFO_FIXTURE_ROOT/commands/iwlist.status"; fi
exit "$code"
''',
}


def canonical(value):
  match value:
    case None:
      return ['null']
    case bool():
      return ['bool', value]
    case int():
      return ['int', str(value)]
    case float():
      if math.isnan(value):
        return ['float', 'nan']
      return ['float', value.hex()]
    case str():
      return ['str', value]
    case list():
      return ['list', [canonical(item) for item in value]]
    case dict():
      return ['dict', [[key, canonical(value[key])] for key in sorted(value)]]
    case _:
      raise AssertionError(type(value))


class Peer:
  """Local datagram peer retaining every actual request and response byte."""

  def __init__(self, endpoint, plans):
    self.socket = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
    self.socket.bind(str(endpoint))
    self.socket.settimeout(0.05)
    self.plans = plans
    self.records = []
    self.failure = None
    self.stop = threading.Event()
    self.ready = threading.Event()
    self.thread = threading.Thread(target=self.run)
    self.thread.start()
    assert self.ready.wait(2)

  def run(self):
    self.ready.set()
    try:
      while not self.stop.is_set():
        try:
          request, address = self.socket.recvfrom(16384)
        except TimeoutError:
          continue
        index = len(self.records)
        assert index < len(self.plans), ('unexpected request', request)
        expected, replies = self.plans[index]
        assert request == expected.encode(), (expected, request)
        assert isinstance(address, bytes) and re.fullmatch(rb'\x00openpilot-wpa-[0-9]+-[0-9]+', address), address
        record = {'request_hex': request.hex(), 'peer_hex': address.hex(), 'replies_hex': []}
        self.records.append(record)
        for response in replies:
          self.socket.sendto(response, address)
          record['replies_hex'].append(response.hex())
    except Exception as error:  # The controller reports every peer failure.
      self.failure = repr(error)

  def finish(self):
    self.stop.set()
    self.thread.join(2)
    self.socket.close()
    assert not self.thread.is_alive()
    assert self.failure is None, self.failure
    assert len(self.records) == len(self.plans), (len(self.records), len(self.plans))
    return self.records


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('binding', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--cases')
  parser.add_argument('--runner', nargs=argparse.REMAINDER, default=[])
  args = parser.parse_args()
  args.binary, args.binding, args.output = (path.resolve() for path in (args.binary, args.binding, args.output))
  args.output.mkdir(parents=True, exist_ok=True)
  provenance = json.loads(args.binding.with_name('provenance.json').read_text())
  assert hashlib.sha256(args.binding.read_bytes()).hexdigest() == provenance['module_sha256']
  for name, digest in provenance['sources'].items():
    assert hashlib.sha256(Path(name).read_bytes()).hexdigest() == digest, name
  cases = build_cases()
  if args.cases:
    cases = [case for case in cases if re.search(args.cases, case.name)]
  assert cases
  rows = []
  comparisons = 0
  packets = 0
  with tempfile.TemporaryDirectory(prefix='hwi98-') as scratch:
    temporary = Path(scratch)
    shims = temporary / 'bin'
    shims.mkdir()
    for name, script in SHIMS.items():
      path = shims / name
      path.write_text(script)
      path.chmod(0o755)
    for index, case in enumerate(cases):
      folder = args.output / case.name
      folder.mkdir()
      results = {}
      invocations = {}
      artifacts = []
      for kind in ('source', 'native'):
        output = folder / kind
        output.mkdir()
        root = output / 'root'
        root.mkdir()
        endpoint = temporary / f'w{index}-{kind}'
        peer = Peer(endpoint, case.wire) if case.wire else None
        if case.endpoint_file:
          endpoint.write_text('not a socket')
        request = {'root': str(root), 'hardware': case.hardware, 'wpa_endpoint': str(endpoint), 'steps': case.steps, 'darwin': case.darwin}
        if case.pc is not None:
          request['pc'] = case.pc
        request_path = output / 'request.json'
        request_path.write_text(json.dumps(request, ensure_ascii=True, indent=2) + '\n')
        command_log = output / 'commands.bin'
        environment = {
          **os.environ,
          'PYTHONPATH': '.:rust/tools',
          'PARAMS_ROOT': str(root / 'params'),
          'PATH': str(shims) + ':' + os.environ['PATH'],
          'HW_INFO_FIXTURE_ROOT': str(root),
          'HW_INFO_COMMAND_LOG': str(command_log),
        }
        environment.pop('OPENPILOT_PREFIX', None)
        command = ([sys.executable, 'rust/tools/hardware_info_reference.py', str(args.binding)] if kind == 'source' else [*args.runner, str(args.binary)]) + [
          str(request_path),
          str(output),
        ]
        start = time.monotonic()
        process = subprocess.run(command, env=environment, capture_output=True, timeout=90)
        elapsed = time.monotonic() - start
        (output / 'stdout.log').write_bytes(process.stdout)
        (output / 'stderr.log').write_bytes(process.stderr)
        wire = peer.finish() if peer else []
        (output / 'wire.json').write_text(json.dumps(wire, indent=2) + '\n')
        raw_commands = command_log.read_bytes() if command_log.exists() else b''
        normalized_commands = raw_commands.replace(str(root).encode(), b'$FIXTURE').hex()
        (output / 'commands.json').write_text(json.dumps({'raw_hex': raw_commands.hex(), 'normalized_hex': normalized_commands}) + '\n')
        invocations[kind] = {'argv': command, 'exit_code': process.returncode, 'elapsed_seconds': elapsed}
        assert process.returncode == 0, (case.name, kind, process.returncode, process.stderr.decode(errors='replace'))
        values = json.loads((output / 'result.json').read_text())
        normalized = [{'method': value['method'], 'value': canonical(json.loads(value['value_json']))} if 'value_json' in value else value for value in values]
        normalized_wire = [{'request_hex': item['request_hex'], 'replies_hex': item['replies_hex']} for item in wire]
        results[kind] = {'values': normalized, 'commands': normalized_commands, 'wire': normalized_wire}
        (output / 'normalized.json').write_text(json.dumps(results[kind], ensure_ascii=True, indent=2) + '\n')
        artifacts.extend(str(output / name) for name in ('request.json', 'result.json', 'normalized.json', 'wire.json', 'commands.json'))
      assert len(results['source']['values']) == sum(step['action'] == 'call' for step in case.steps), case.name
      if results['source'] != results['native']:
        differing = [key for key in results['source'] if results['source'][key] != results['native'][key]]
        details = {'result': 'FAIL', 'case': case.name, 'differing': differing, 'source': results['source'], 'native': results['native']}
        (folder / 'failure.json').write_text(json.dumps(details, ensure_ascii=True, indent=2) + '\n')
        raise AssertionError((case.name, differing, str(folder / 'failure.json')))
      count = len(results['source']['values'])
      row = {
        'scenario': case.name,
        'result': 'PASS',
        'calls': count,
        'invocations': invocations,
        'packets': len(case.wire),
        'binary_observable': 'Exact Python scalar types/values and exception categories; real command order/arguments and Unix request/reply bytes agree',
        'artifacts': artifacts,
      }
      (folder / 'comparison.json').write_text(json.dumps(row, indent=2) + '\n')
      rows.append(row)
      comparisons += count
      packets += len(case.wire)
      print(case.name, 'PASS', count, flush=True)
  sources = [
    'openpilot/system/hardware/__init__.py',
    'openpilot/system/hardware/hw.py',
    'openpilot/system/hardware/base.py',
    'openpilot/system/hardware/pc/hardware.py',
    'openpilot/system/hardware/tici/hardware.py',
    'openpilot/system/hardware/tici/iwlist.py',
    'openpilot/common/utils.py',
    'openpilot/common/params_pyx.pyx',
    'openpilot/common/params.cc',
    'openpilot/common/params.h',
  ]
  report = {
    'result': 'PASS',
    'scenario_count': len(rows),
    'comparisons': comparisons,
    'datagram_requests_per_implementation': packets,
    'selection': args.cases,
    'scenarios': rows,
    'source_sha256': {name: hashlib.sha256(Path(name).read_bytes()).hexdigest() for name in sources},
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'binding_sha256': hashlib.sha256(args.binding.read_bytes()).hexdigest(),
    'scope': 'Read-only information/Paths with fixture files, commands and local sockets; no board writes, real modem, AGNOS or vehicle validation.',
  }
  (args.output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({key: report[key] for key in ('result', 'scenario_count', 'comparisons', 'datagram_requests_per_implementation', 'selection')}), flush=True)


if __name__ == '__main__':
  main()
