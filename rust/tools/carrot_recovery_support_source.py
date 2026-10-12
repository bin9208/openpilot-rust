from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import threading


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--port', type=int, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  root = args.output.resolve()
  source = Path('openpilot/selfdrive/carrot/recovery/server.py').resolve()
  spec = importlib.util.spec_from_file_location('owned_recovery_support_source', source)
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  module.PARAMS_DIR = str(root / 'fixture-params')
  module.REPO_ROOT = root / 'openpilot'
  module.TMUX_LOG_PATH = str(root / 'tmux.log')
  server = module.ThreadingHTTPServer(('127.0.0.1', args.port), module.RecoveryHandler)
  server.daemon_threads = True
  root.joinpath('source-proof.json').write_text(
    json.dumps({
      'source': str(source), 'sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
      'runtime_functions': 'unchanged actual RecoveryHandler/WsConn/RecoveryPty/SupportManager/_read_param',
      'fixture_constants': {'PARAMS_DIR': module.PARAMS_DIR, 'REPO_ROOT': str(module.REPO_ROOT), 'TMUX_LOG_PATH': module.TMUX_LOG_PATH},
      'source_main_cleanup_not_claimed': True,
    }, indent=2) + '\n'
  )
  worker = threading.Thread(target=server.serve_forever)
  worker.start()
  try:
    sys.stdin.readline()
  finally:
    server.shutdown()
    server.server_close()
    worker.join()
    module.PTY.terminate()
    module.SUPPORT.stop('owned_fixture_cleanup')


if __name__ == '__main__':
  main()
