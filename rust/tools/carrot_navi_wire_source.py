from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys

from openpilot.cereal import log
from openpilot.selfdrive.carrot.carrot_navi_cereal import build_carrot_navi_payload


def main() -> None:
  input_path, output_path = map(Path, sys.argv[1:])
  snapshots = json.loads(input_path.read_text())
  rows = []
  for snapshot in snapshots:
    event = log.Event.new_message(valid=True, logMonoTime=777)
    event.init('carrotNavi')
    try:
      event.carrotNavi = build_carrot_navi_payload(snapshot, publish_mono_ns=999)
      rows.append({'wire_hex': event.to_bytes().hex(), 'error': None})
    except (ValueError, TypeError, OverflowError, UnicodeError) as error:
      rows.append({'wire_hex': None, 'error': {'type': type(error).__name__, 'message': str(error)}})
  output_path.write_text(json.dumps(rows, ensure_ascii=True, indent=2))
  source = Path(sys.modules[build_carrot_navi_payload.__module__].__file__)
  output_path.with_suffix('.invocation.json').write_text(json.dumps({'argv': sys.argv,
    'python': sys.version, 'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
    'timestamp_seam': 'explicit Event777 and helper publish999', 'cases': len(rows)}, indent=2))


if __name__ == '__main__':
  main()
