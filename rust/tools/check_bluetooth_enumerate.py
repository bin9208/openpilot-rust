import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from pytest import MonkeyPatch
from bluetooth_engine_source import daemon


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  root = args.output.resolve()
  sysfs, devices = root / 'sysfs', root / 'dev'
  sysfs.mkdir()
  cases = [
    ('event0', b'0005', b'aa:bb:cc:dd:ee:ff', b'remote'),
    ('event9', b'0003', b'AA:BB:CC:DD:EE:01', b'USB'),
    ('event2', b'0005', b'invalid', b'remote'),
    ('event3', b'0005', b'AA:BB:CC:DD:EE:02', None),
    ('event4', b'0005', b'AA:BB:CC:DD:EE:03', b'\xff'),
    ('mouse0', b'0005', b'AA:BB:CC:DD:EE:04', b'remote'),
    ('event5', None, b'AA:BB:CC:DD:EE:05', b'remote'),
    ('event6', b'0005', None, b'remote'),
    ('event7', b'\xff', b'AA:BB:CC:DD:EE:07', b'remote'),
    ('event8', b'0005', b'\xff', b'remote'),
    ('event', b'0005', b'AA:BB:CC:DD:EE:08', b''),
  ]
  whitespace = [chr(value) for value in range(0x110000) if chr(value).isspace()]
  for index, space in enumerate(whitespace):
    cases.append((f'event-space-{index}', f'{space}0005{space}'.encode(),
                  f'{space}aa:bb:cc:dd:ee:ff{space}'.encode(), f'{space}remote{space}'.encode()))
  for name, bus, address, label in cases:
    directory = sysfs / name / 'device'
    (directory / 'id').mkdir(parents=True)
    for relative, data in [('id/bustype', bus), ('uniq', address), ('name', label)]:
      if data is not None:
        (directory / relative).write_bytes(data)

  def redirected_path(path: str) -> Path:
    return {'/sys/class/input': sysfs, '/dev/input': devices}.get(path, Path(path))

  with MonkeyPatch.context() as patch:
    patch.setattr(daemon, 'Path', redirected_path)
    expected = {path: value[0] for path, value in daemon.devices().items()}
  native = subprocess.run([str(args.binary.resolve()), 'enumerate', str(sysfs), str(devices)],
                          text=True, capture_output=True, check=True, timeout=5)
  observed = json.loads(native.stdout)
  assert list(expected.items()) == list(observed.items()), (expected, observed)
  assert len(expected) == len(whitespace) + 2
  absent = subprocess.run([str(args.binary.resolve()), 'enumerate', str(root / 'absent'), str(devices)],
                          text=True, capture_output=True, check=True, timeout=5)
  assert json.loads(absent.stdout) == {}
  result = {'nodes': len(cases), 'accepted': len(expected), 'source': expected, 'native': observed,
            'source_sha256': hashlib.sha256(Path(daemon.__file__).read_bytes()).hexdigest(),
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (root / 'result.json').write_text(json.dumps(result, indent=2))
  print(f'PASS: {len(cases)} sysfs nodes, {len(expected)} accepted in source order, missing directory')


if __name__ == '__main__':
  main()
