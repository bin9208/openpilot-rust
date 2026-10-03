import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess
from typing import TypeAlias, assert_never

from bluetooth_engine_scenarios import scenarios
from bluetooth_engine_source import Source

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']


def normalize(value: Json, ids: dict[str, int]) -> Json:
  match value:
    case dict():
      result: dict[str, Json] = {}
      for key, child in value.items():
        if key == 'id' and isinstance(child, str):
          if ':' in child:
            assert re.fullmatch('[0-9a-f]{32}:[1-9][0-9]*', child)
            result[key] = child.split(':')[1]
          else:
            assert re.fullmatch('[0-9a-f]{32}', child)
            if child not in ids:
              ids[child] = len(ids)
            result[key] = ids[child]
        elif key in ('cruise', 'lane', 'status') and isinstance(child, str):
          result[key] = normalize(json.loads(child), ids)
        else:
          result[key] = normalize(child, ids)
      return result
    case list():
      return [normalize(child, ids) for child in value]
    case float() if not math.isfinite(value):
      return str(value)
    case None | bool() | int() | float() | str():
      return value
    case unreachable:
      assert_never(unreachable)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  total = 0
  summary = []
  for name, steps in scenarios():
    directory = args.output / name
    directory.mkdir()
    source = directory / 'source'
    source.mkdir()
    expected = Source(source, steps).run()
    payload = ''.join(json.dumps(step) + '\n' for step in steps)
    (directory / 'input.jsonl').write_text(payload)
    native = subprocess.run([str(args.binary.resolve()), str((directory / 'native').resolve())], input=payload, text=True, capture_output=True, timeout=30)
    (directory / 'native.jsonl').write_text(native.stdout)
    (directory / 'native.stderr').write_text(native.stderr)
    (directory / 'source.json').write_text(json.dumps(expected, indent=2))
    native.check_returncode()
    actual = [json.loads(line) for line in native.stdout.splitlines()]
    left, right = normalize(expected, {}), normalize(actual, {})
    (directory / 'normalized-source.json').write_text(json.dumps(left, indent=2))
    (directory / 'normalized-native.json').write_text(json.dumps(right, indent=2))
    assert left == right, name
    summary.append({'scenario': name, 'steps': len(steps)})
    total += len(steps)
  result = {
    'cases': len(summary),
    'steps': total,
    'exact': 'command journals, status histories, reload/open/close decisions',
    'normalization': 'random UUID identity only; nonfinite values compared symbolically',
    'scenarios': summary,
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
  }
  (args.output / 'result.json').write_text(json.dumps(result, indent=2))
  print(json.dumps(result))


if __name__ == '__main__':
  main()
