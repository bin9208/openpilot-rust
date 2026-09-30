# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "zstandard==0.25.0"]
# ///
# Launched by check_model_startup.py with the original native msgq/VisionIPC bindings.
"""Execute the original camera connection and model initialization statements unchanged."""
from __future__ import annotations

import argparse
import ast
import json
import logging
import os
from pathlib import Path
import pickle
import time
from types import SimpleNamespace

import numpy as np
from tinygrad import Tensor
from msgq.visionipc import VisionIpcClient, VisionStreamType, VisionBuf

from openpilot.common.file_chunker import open_file_chunked
from openpilot.selfdrive.modeld import helpers
from openpilot.selfdrive.modeld.camera_sync import receive_camera_pair
from openpilot.selfdrive.modeld.compile_modeld import make_input_queues, WARP_INPUTS, POLICY_INPUTS
from openpilot.selfdrive.modeld.constants import ModelConstants
from openpilot.selfdrive.modeld.parse_model_outputs import Parser
from openpilot.system.camerad.cameras.nv12_info import get_nv12_info


class Events(logging.Handler):
  def emit(self, record: logging.LogRecord) -> None:
    print(json.dumps({'event': 'log', 'message': record.getMessage(), 'level': record.levelno, 'time': time.monotonic()}), flush=True)


def run(component: str, models: Path) -> None:
  root = Path(__file__).resolve().parents[2]
  path = root / f'openpilot/selfdrive/modeld/{component}.py'
  tree = ast.parse(path.read_text())
  model = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'ModelState')
  main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
  helpers.MODELS_DIR = models
  helpers.TG_INPUT_DEVICES_PATH = models / 'tg_input_devices.json'
  logger = logging.getLogger('source_startup')
  logger.setLevel(logging.DEBUG)
  logger.addHandler(Events())
  scope = {
    'np': np, 'pickle': pickle, 'time': time, 'Tensor': Tensor, 'VisionIpcClient': VisionIpcClient,
    'VisionStreamType': VisionStreamType, 'VisionBuf': VisionBuf, 'open_file_chunked': open_file_chunked,
    'make_input_queues': make_input_queues, 'WARP_INPUTS': WARP_INPUTS, 'POLICY_INPUTS': POLICY_INPUTS,
    'ModelConstants': ModelConstants, 'Parser': Parser, 'get_nv12_info': get_nv12_info, 'SEND_RAW_PRED': os.getenv('SEND_RAW_PRED'),
    'cloudlog': logger, 'PROCESS_NAME': f'openpilot.selfdrive.modeld.{component}',
    'get_tg_input_devices': helpers.get_tg_input_devices, 'modeld_pkl_path': helpers.modeld_pkl_path,
    'load_oob': helpers.load_oob, 'select_vision_streams': helpers.select_vision_streams, 'MODELS_DIR': models,
    'MODEL_PKL_PATH': models / 'dmonitoring_model_tinygrad.pkl', 'METADATA_PATH': models / 'dmonitoring_model_metadata.pkl',
  }
  exec(compile(ast.Module(body=[model], type_ignores=[]), str(path), 'exec'), scope)
  match component:
    case 'modeld':
      start = next(i for i, node in enumerate(main.body) if isinstance(node, ast.While))
      end = next(i for i, node in enumerate(main.body) if ast.unparse(node).startswith("cloudlog.warning(f'models loaded in"))
      scope.update(use_wide_camera=True, USBGPU=False,
                   params=SimpleNamespace(put_bool=lambda key, value: (Path(os.environ['PARAMS_ROOT']) /
                     os.environ['OPENPILOT_PREFIX'] / key).write_bytes(b'1' if value else b'0')))
    case 'dmonitoringmodeld':
      start = next(i for i, node in enumerate(main.body) if ast.unparse(node).startswith("cloudlog.warning('connecting"))
      end = next(i for i, node in enumerate(main.body) if ast.unparse(node).startswith("cloudlog.warning('models loaded"))
    case _:
      raise AssertionError(component)
  nodes = main.body[start:end + 1]
  wrapper = ast.parse('def startup():\n  return locals()').body[0]
  wrapper.body = [*nodes, wrapper.body[-1]]
  exec(compile(ast.fix_missing_locations(ast.Module(body=[wrapper], type_ignores=[])), str(path), 'exec'), scope)
  scope.update(scope['startup']())
  client = scope['vipc_client_main' if component == 'modeld' else 'vipc_client']
  result = {'event': 'initialized', 'component': component, 'time': time.monotonic(), 'width': client.width, 'height': client.height,
            'buffer_len': client.buffer_len, 'source': str(path), 'source_lines': [nodes[0].lineno, nodes[-1].end_lineno],
            'model_class': type(scope['model']).__name__, 'frames_sent_before_initialization': 0,
            'scope': 'original ModelState plus original connection/load statements; artifact directory redirected; internal model only'}
  print(json.dumps(result), flush=True)
  assert input() == 'receive'
  if component == 'modeld':
    frames = None
    while frames is None:
      frames = receive_camera_pair(client, scope['vipc_client_extra'] if scope['use_extra_client'] else None)
    _main, metadata, _extra, _extra_metadata = frames
    frame_id = metadata.frame_id
  else:
    frame = None
    while frame is None:
      frame = client.recv()
    frame_id = client.frame_id
  print(json.dumps({'event': 'first_frame', 'frame_id': frame_id, 'time': time.monotonic()}), flush=True)


if __name__ == '__main__':
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--component', choices=['modeld', 'dmonitoringmodeld'], required=True)
  parser.add_argument('--models', type=Path, required=True)
  args = parser.parse_args()
  run(args.component, args.models.resolve())
