"""Fit one build's hard limit below the device policy and real headroom."""
from pathlib import Path

MIB = 1024**2
MAXIMUM = (1 << 64)-1


def number(value):
  if not value.isascii() or not value.isdecimal() or int(value) > MAXIMUM:
    raise ValueError('invalid native build memory counter')
  return int(value)


def fit(policy, meminfo=Path('/proc/meminfo')):
  maximum = policy['maximum_memory_bytes']
  reserve = policy['minimum_available_memory_bytes']
  if (any(type(value) is not int or value <= 0 for value in (maximum, reserve))
      or maximum+reserve > MAXIMUM):
    raise ValueError('native build memory policy needs positive bounded limits')
  rows = [line.split()[1:] for line in meminfo.read_text().splitlines() if line.startswith('MemAvailable:')]
  if len(rows) != 1 or len(rows[0]) != 2 or rows[0][1] != 'kB':
    raise ValueError('native build needs kernel MemAvailable in kB')
  host = number(rows[0][0])*1024
  if host > MAXIMUM:
    raise ValueError('available native build memory overflow')
  available = host
  ancestors = []
  parent = Path(policy['memory_cgroup_parent'])
  # The private controller can be created by the seed after this check. Include
  # its existing ancestors now, using the same reclaimable-file rule as native.
  while not parent.exists():
    parent = parent.parent
  while True:
    if parent.resolve(strict=True) != parent or parent.is_symlink():
      raise ValueError('native build memory controller is redirected')
    if (parent/'memory.max').is_file():
      limit_name, usage_name = 'memory.max', 'memory.current'
    elif (parent/'memory.limit_in_bytes').is_file():
      limit_name, usage_name = 'memory.limit_in_bytes', 'memory.usage_in_bytes'
    else:
      if parent == parent.parent:
        break
      parent = parent.parent
      continue
    raw = (parent/limit_name).read_text().strip()
    if raw != 'max':
      limit = number(raw)
      usage = number((parent/usage_name).read_text().strip())
      inactive = [number(value) for key, value in
          (line.split(maxsplit=1) for line in (parent/'memory.stat').read_text().splitlines())
          if key in ('inactive_file', 'total_inactive_file')]
      reclaimable = min(usage, max(inactive, default=0))
      headroom = max(0, limit-(usage-reclaimable))
      ancestors.append(dict(path=str(parent), headroom_bytes=headroom))
      available = min(available, headroom)
    if parent == parent.parent:
      break
    parent = parent.parent
  # Keep the normal reserve and another reserve's worth of startup headroom.
  # Round reductions down so process startup does not immediately hit the old
  # guard again. This is a smaller enforced cap, never an admission bypass.
  selected = min(maximum, max(0, available-2*reserve))
  if selected < maximum:
    selected = selected//(64*MIB)*(64*MIB)
  if selected <= 0:
    raise ValueError(f'insufficient memory for a bounded native build; available_bytes={available} '
                     f'normal_reserve_bytes={reserve} configured_maximum_bytes={maximum}')
  report = dict(schema=1, host_available_bytes=host, available_bytes=available, ancestors=ancestors,
      configured_maximum_bytes=maximum, maximum_memory_bytes=selected,
      normal_reserve_bytes=reserve, startup_headroom_bytes=available-selected-reserve)
  return dict(policy, maximum_memory_bytes=selected), report
