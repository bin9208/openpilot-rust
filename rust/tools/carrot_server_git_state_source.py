from __future__ import annotations

import ast
import json
from pathlib import Path
import sys
import types


def original(directory: Path):
  path = Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server/services/git_state.py'
  tree = ast.parse(path.read_text(), filename=str(path))
  tree.body = [node for node in tree.body if not isinstance(node, ast.ImportFrom) or node.level == 0]
  module = types.ModuleType('original_git_state')
  module.CARROT_STATE_DIR = str(directory)
  module.CARROT_GIT_STATE_PATH = str(directory / 'git.json')
  exec(compile(tree, str(path), 'exec'), module.__dict__)
  return module


def main() -> None:
  config = json.loads(sys.stdin.readline())
  module = original(Path(config['directory']))
  for line in sys.stdin:
    step = json.loads(line)
    module.time = types.SimpleNamespace(time=lambda: step.get('seconds'), time_ns=lambda: step.get('nanoseconds'))
    try:
      match step['operation']:
        case 'read': result = module.read_git_state()
        case 'write': result = module.write_git_state(step['data'])
        case 'meta': result = module.read_custom_meta_value(step['name'])
        case 'pull_time': result = module.write_git_pull_time(step.get('timestamp'))
        case 'auto_read': result = module.read_auto_update_state()
        case 'event': result = module.write_auto_update_event(step['status'], **step['fields'])
        case 'did_pull': result = module.did_git_pull_update(step['output'])
        case operation: raise ValueError(f'unknown owned Git state operation: {operation}')
      output = {'result': result}
    except Exception as error:
      output = {'exception': type(error).__name__, 'message': str(error)}
    print(json.dumps(output, ensure_ascii=True), flush=True)


if __name__ == '__main__':
  main()
