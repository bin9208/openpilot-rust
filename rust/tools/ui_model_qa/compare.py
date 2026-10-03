"""Compare complete ordered drawing and state trees at the fixed numeric budget."""

import json
import math
from pathlib import Path
import sys


def differences(a, b, path='$'):
  if isinstance(a, dict) and isinstance(b, dict):
    if a.keys() != b.keys():
      yield path, 'keys', sorted(a), sorted(b)
    for key in a.keys() & b.keys():
      yield from differences(a[key], b[key], f'{path}.{key}')
  elif isinstance(a, list) and isinstance(b, list):
    if len(a) != len(b):
      yield path, 'length', len(a), len(b)
    for i, (x, y) in enumerate(zip(a, b, strict=False)):
      yield from differences(x, y, f'{path}[{i}]')
  elif isinstance(a, (int, float)) and isinstance(b, (int, float)) and not isinstance(a, bool):
    exact = isinstance(a, int) or any(token in path for token in ('.commands[', '.state.common.', '.state.carrot.barriers[', '.state.marking_segments['))
    if (a != b) if exact else not math.isclose(a, b, rel_tol=1e-11, abs_tol=1e-11):
      yield path, 'number', a, b
  elif a != b:
    yield path, 'value', a, b


if __name__ == '__main__':
  a, b = (json.loads(Path(p).read_text()) for p in sys.argv[1:3])
  errors = list(differences(a, b))
  print(json.dumps({'passed': not errors, 'count': len(errors), 'differences': errors[:30]}, indent=2))
  sys.exit(bool(errors))
