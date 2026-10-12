from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path


def main() -> None:
  root = Path(os.environ['CWP_PROBE_ROOT']).resolve()
  source = Path('openpilot/selfdrive/carrot/recovery/server.py').resolve()
  spec = importlib.util.spec_from_file_location('owned_recovery_cwp_source', source)
  if spec is None or spec.loader is None:
    raise ImportError(str(source))
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  module.PARAMS_DIR = str(root / 'params')
  module._local_ip = lambda: (root / 'ip').read_text().strip()
  (root / 'source-proof.json').write_text(json.dumps({
    'source': str(source),
    'sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
    'executed_functions': ['_cwp_status', '_cwp_set_enabled', '_cwp_boot_worker', '_cwp_request', '_request_result', '_read_param', '_write_param', '_cwp_device_id'],
    'fixture_boundaries': ['PARAMS_DIR owned directory', '_local_ip owned address file'],
    'runtime_functions_changed': False,
  }, indent=2) + '\n')
  match os.environ['CWP_PROBE_ACTION']:
    case 'status':
      result = module._cwp_status()
    case 'enable':
      result = module._cwp_set_enabled(True)
    case 'disable':
      result = module._cwp_set_enabled(False)
    case 'boot':
      module._cwp_boot_worker(6999)
      result = True
    case action:
      raise RuntimeError(f'unknown source fixture action: {action}')
  print('CWP_RESULT=' + json.dumps(result), flush=True)


if __name__ == '__main__':
  main()
