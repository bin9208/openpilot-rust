import argparse
from collections import Counter, defaultdict
from hashlib import sha256
import json
from pathlib import Path
import re

from openpilot.cereal import log


def canonical(value):
  return json.dumps(value, sort_keys=True, default=lambda value: value.hex() if isinstance(value, bytes) else str(value))


def transitions(rows):
  result = []
  for row in rows:
    encoded = canonical(row)
    if not result or result[-1] != encoded:
      result.append(encoded)
  return result


def log_payload(payload):
  message = payload['msg']
  if isinstance(message, dict) and 'timestamp' in message:
    message = dict(message, timestamp='RUN_TIME')
  elif isinstance(message, str):
    match = re.fullmatch(r'sendcan too old to send: (\d+), (\d+)', message)
    if match:
      now, sent = map(int, match.groups())
      age = 'future' if sent > now else 'stale' if now - sent >= 1_000_000_000 else 'fresh'
      message = {'diagnostic': 'sendcan too old to send', 'age': age}
  result = {'levelnum': payload['levelnum'], 'msg': message}
  if payload['filename'].startswith('panda['):
    result['filename'] = payload['filename']
  return result


def messages(path, service):
  if not path.exists():
    return []
  rows = []
  for event in log.Event.read_multiple_bytes(path.read_bytes()):
    assert event.which() == service
    value = event.to_dict()
    value.pop('logMonoTime')
    if service in ('logMessage', 'errorLogMessage'):
      value[service] = log_payload(json.loads(value[service]))
    rows.append(value)
  return rows


def usb(path):
  outputs = defaultdict(list)
  can_health_reads = defaultdict(list)
  for line in path.read_text().splitlines():
    row = json.loads(line)
    if row['op'] == 'write':
      outputs['CAN_WRITE'].append(row)
    elif row['op'] == 'control':
      key = f"CONTROL:{row['device']}:{row['kind']}:{row['request']}"
      if row['kind'] & 0x80:
        key += f":{row['value']}:{row['index']}"
        if row['request'] == 0xc2:
          can_health_reads[row['device']].append(row['value'])
      outputs[key].append(row)
  for values in can_health_reads.values():
    assert len(values) % 3 == 0 and values == [0, 1, 2] * (len(values) // 3), values
  return {key: [canonical(row) for row in rows] if key == 'CAN_WRITE' else transitions(rows)
          for key, rows in outputs.items()}


def main():
  parser = argparse.ArgumentParser(description='Compare captured original and Rust Panda daemon publications and USB effects.')
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--native', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  source_manifest = json.loads((args.source / 'manifest.json').read_text())
  native_manifest = json.loads((args.native / 'manifest.json').read_text())
  assert source_manifest['result'] == native_manifest['result'] == 'PASS'
  names = [row['scenario'] for row in source_manifest['scenarios']]
  assert names == [row['scenario'] for row in native_manifest['scenarios']]
  hashes = {}
  results = []
  for name in names:
    source = args.source / name
    native = args.native / name
    compared = {}
    for service in ('can', 'pandaStates', 'peripheralState', 'logMessage', 'errorLogMessage'):
      rows = [messages(root / f'{service}.bin', service) for root in (source, native)]
      if name in ('firmware-mismatch', 'malformed-params'):
        continue
      if service in ('logMessage', 'errorLogMessage'):
        expected, actual = (Counter(canonical(row) for row in values) for values in rows)
      else:
        expected, actual = (transitions(values) for values in rows)
      assert expected == actual, (name, service, expected, actual)
      compared[service] = {'source_count': len(rows[0]), 'native_count': len(rows[1]), 'distinct_transitions': len(expected)}
    expected_usb = usb(source / 'usb-trace.jsonl')
    actual_usb = usb(native / 'usb-trace.jsonl')
    assert expected_usb == actual_usb, (name, expected_usb, actual_usb)
    for root in (source, native):
      for pattern in ('*.bin', 'usb-trace.jsonl', 'invocation.json'):
        for path in root.glob(pattern):
          hashes[str(path.resolve())] = sha256(path.read_bytes()).hexdigest()
    results.append({'scenario': name, 'result': 'PASS', 'messages': compared, 'usb_channels': len(expected_usb)})
  report = {'result': 'PASS', 'scenarios': results,
            'source_binary_sha256': source_manifest['binary_sha256'], 'native_binary_sha256': native_manifest['binary_sha256'],
            'capture_sha256': hashes,
            'normalization': ['Exclude event clocks and logging process/callsite metadata.',
                              'Compare publication transitions without repeated identical periodic samples.',
                              'Compare log payload multiplicity without cross-thread order; classify send age and exclude trace clock values.',
                              'Compare changes per USB control-write request and control-read request/value/index.',
                              'Require complete ordered CAN-health bus cycles.',
                              'Compare the complete ordered CAN-write list.',
                              'Fatal cases compare USB effects; native typed failures and original abort diagnostics differ.'],
            'limits': 'Independent host schedules are not a deterministic timing comparison or a device measurement.'}
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'result': 'PASS', 'scenarios': len(results), 'report': str(args.output)}))


if __name__ == '__main__':
  main()
