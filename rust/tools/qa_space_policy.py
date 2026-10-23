"""Observed operation-growth admission for host QA; no fixed runner reserve (#509)."""
from __future__ import annotations

import json
from pathlib import Path
import shutil
import sys


def free_for_growth(path: Path, growth: int) -> int:
  """Record the real observation; callers retain their existing failure branches."""
  free = shutil.disk_usage(path).free
  if growth < 0:
    raise ValueError('estimated growth must be nonnegative')
  receipt = {'path': str(path), 'free_bytes': free, 'growth_bytes': growth,
             'reserve_bytes': 0, 'required_bytes': growth, 'admitted': free >= growth}
  print(json.dumps(receipt), file=sys.stderr, flush=True)
  return free
