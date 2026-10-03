import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
import subprocess

from bluetooth_engine_source import daemon


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  rng = random.Random(15564)
  records = [(sec, usec, 65535, 65535, -(2**31)) for sec in (-(2**63), -1, 0, 1, 2**63 - 1)
             for usec in (-(2**63), -1, 0, 999999, 2**63 - 1)]
  records.extend((rng.randrange(-(2**63), 2**63), rng.randrange(-(2**63), 2**63), rng.randrange(65536),
                  rng.randrange(65536), rng.randrange(-(2**31), 2**31)) for _ in range(10000))
  data = [daemon.EVENT.pack(*record) for record in records]
  data.extend(bytes(length) for length in range(24))
  payload = ''.join(json.dumps(list(value)) + '\n' for value in data)
  (args.output / 'input.jsonl').write_text(payload)
  native = subprocess.run([str(args.binary.resolve()), 'decode'], input=payload, text=True, capture_output=True, timeout=30)
  (args.output / 'native.jsonl').write_text(native.stdout)
  (args.output / 'native.stderr').write_text(native.stderr)
  native.check_returncode()
  actual = [json.loads(line) for line in native.stdout.splitlines()]
  expected = []
  for raw, observed in zip(data, actual, strict=True):
    if not raw or len(raw) % daemon.EVENT.size:
      value = {'error': 'HID device disconnected or incomplete event'}
    else:
      value = {'events': [{'kind': kind, 'code': code, 'value': value, 'at': sec + usec / 1e6}
                          for sec, usec, kind, code, value in daemon.EVENT.iter_unpack(raw)]}
      for source_event, native_event in zip(value['events'], observed['events'], strict=True):
        assert struct.pack('>d', source_event['at']) == struct.pack('>d', native_event['at'])
    assert value == observed
    expected.append(value)
  (args.output / 'source.json').write_text(json.dumps(expected))
  result = {'cases': len(data), 'exact': 'signed fields and timestamp binary64 bits',
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
            'source_sha256': hashlib.sha256(Path(daemon.__file__).read_bytes()).hexdigest()}
  (args.output / 'result.json').write_text(json.dumps(result, indent=2))
  print(json.dumps(result))


if __name__ == '__main__':
  main()
