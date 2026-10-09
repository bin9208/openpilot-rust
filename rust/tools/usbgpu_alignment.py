"""Assert the actual pinned gfx1200 startup configuration before adapting stores."""

from __future__ import annotations

import json
from pathlib import Path

from usbgpu_global_store import GlobalStoreContractError


def configured_mode(trace, evidence: Path) -> int:
  metadata = Path(__file__).resolve().parents[1] / 'crates/usbgpu/assets/amd-metadata.json'
  catalog = json.loads(metadata.read_text())
  offset, segment, fields = catalog['registers']['gc_12_0_0']['regSH_MEM_CONFIG']
  base = catalog['constants']['navi_offsets'][f'GC_BASE__INST0_SEG{segment}']
  low, high = fields['alignment_mode']
  mask = (1 << (high - low + 1)) - 1
  writes = [
    {'trace_offset': index, **row, 'alignment_mode': (row['value'] >> low) & mask}
    for index, row in enumerate(trace)
    if row.get('op') == 'reg_write' and row.get('index') == base + offset
  ]
  unaligned = catalog['constants']['soc_12']['SH_MEM_ALIGNMENT_MODE_UNALIGNED']
  passed = unaligned == 3 and len(writes) == 16 and all(row['alignment_mode'] == unaligned for row in writes)
  result = {
    'register_index': base + offset,
    'alignment_mode': unaligned,
    'writes': writes,
    'passed': passed,
    'scope': 'Actual pinned gfx1200 startup writes, all sixteen VMIDs; adapter is not enabled for DWORD modes.',
  }
  (evidence / 'global-store-config.json').write_text(json.dumps(result, indent=2) + '\n')
  if not passed:
    raise GlobalStoreContractError('actual startup did not configure all VMIDs for UNALIGNED mode')
  return unaligned
