"""Recover actual native Params file reads between renderer-owned scope markers."""

import re


def reads(path):
  result = {}
  active = None
  for line in path.read_text().splitlines():
    start = re.search(r'MODEL_PARAMS_BEGIN ([^\\]+)\\n', line)
    stop = re.search(r'MODEL_PARAMS_END ([^\\]+)\\n', line)
    if start:
      active = start.group(1)
      if active in result:
        raise AssertionError(f'duplicate scope {active}')
      result[active] = []
    elif stop:
      assert active == stop.group(1), (active, line)
      active = None
    elif active and 'O_RDONLY' in line:
      match = re.search(r'/params/d/([^"/]+)"', line)
      if match:
        result[active].append(match.group(1))
  assert active is None, active
  return result
