# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Original seven CLI handlers and vision runner, with actual Params/VIPC on owned roots."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import resource
import sys
from types import ModuleType

from carrot_server_dashcam_upload import save
from original_params_binding import load


def configure(path: Path):
  config = json.loads(path.read_text())
  root = Path(config['owned_root']).resolve()
  assert Path(os.environ['PARAMS_ROOT']).is_relative_to(root)
  assert os.environ['OPENPILOT_PREFIX'] == config['prefix']
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  feature_root = Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server/features'
  for name, directory in [('features', feature_root), ('features.dashcam', feature_root / 'dashcam')]:
    module = ModuleType('openpilot.selfdrive.carrot.server.' + name)
    module.__path__ = [str(directory)]
    sys.modules[module.__name__] = module
  load(os.environ['ORIGINAL_PARAMS_BINDING'], f'ipc://{root}/owned-log', root / 'logs')
  import msgq

  msgq.__path__.insert(0, config['vision_root'])
  from openpilot.selfdrive.carrot.server.terminal_commands import cli, registry
  from openpilot.selfdrive.carrot.server.services import vision_test as vision, web_settings

  vision.REPO_ROOT = root / 'repository'
  vision.STATE_PATH = root / 'vision-state.json'
  vision.LOG_PATH = root / 'vision.log'
  vision.RUNNER_MODULE = 'carrot_server_terminal_command_source'
  vision._PROCESS_SPECS = {
    'camerad': {'cmd': [config['camera']], 'cwd': str(vision.REPO_ROOT), 'match': config['camera']},
    'stream_encoderd': {'cmd': [config['encoder']], 'cwd': str(vision.REPO_ROOT), 'match': config['encoder']},
    'webrtcd': {'cmd': [config['webrtc']], 'cwd': str(vision.REPO_ROOT), 'match': config['webrtc']},
  }
  web_settings.CARROT_WEB_SETTINGS_PATH = str(root / 'state/web_settings.json')
  web_settings.DRIVE_CONTENT_CATALOG_PATH = str(root / 'content-catalog.json')
  # Bind only the source's fixed local port literal in these four functions.
  import ast
  from carrot_server_tools_source import OwnedPaths

  source = Path(vision.__file__)
  tree = ast.parse(source.read_text())
  providers = OwnedPaths({'5001': str(config['port'])})
  tree = providers.visit(tree)
  for node in ast.walk(tree):
    if isinstance(node, ast.Constant) and node.value == 5001:
      node.value = config['port']
  functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in {'get_status', 'print_status', '_run_test'}]
  exec(compile(ast.Module(body=functions, type_ignores=[]), str(source), 'exec'), vision.__dict__)
  save(
    root / 'cli-source-proof.json',
    {
      'original_files': {str(Path(module.__file__)): hashlib.sha256(Path(module.__file__).read_bytes()).hexdigest() for module in [cli, registry, vision]},
      'bound_fixed_port': config['port'],
      'function_bodies_preserved_except_port_literals': True,
      'actual_params_root': vision._params().get_param_path(),
      'vision_root': config['vision_root'],
    },
  )
  return cli, vision


def main() -> None:
  args = sys.argv[1:]
  if args[:1] == ['--config']:
    path = Path(args[1]).resolve()
    os.environ['OWNED_TERMINAL_CONFIG'] = str(path)
    args = args[2:]
  else:
    path = Path(os.environ['OWNED_TERMINAL_CONFIG'])
  cli, vision = configure(path)
  if args[:1] == ['_run']:
    code = vision.main(args)
  else:
    code = cli.main(args)
  raise SystemExit(code)


if __name__ == '__main__':
  main()
