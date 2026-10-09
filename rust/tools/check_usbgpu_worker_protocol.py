from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

import numpy as np


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--metadata', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(exist_ok=True, parents=True)
  sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
  from openpilot.selfdrive.modeld import precompiled_runner as original

  popen = subprocess.Popen
  rows = []
  os.environ['SEND_RAW_PRED'] = '1'
  for width, height in [(1344, 760), (1928, 1208)]:
    log = args.evidence / f'inputs-{width}x{height}.jsonl'
    invocations = []

    def launch(command, log=log, invocations=invocations, **kwargs):
      native = [str(args.binary), str(args.metadata), *command[-3:], str(log)]
      invocations.append(native)
      return popen(native, **kwargs)

    original.subprocess.Popen = launch
    model = original.PrecompiledModelState(width, height, args.metadata)
    shared_path = Path(model.file.name)
    process = model.process
    frames = {name: SimpleNamespace(data=np.arange(model.frame_size, dtype=np.uint8)) for name in ('img', 'big_img')}
    transforms = {'img': np.eye(3, dtype=np.float32), 'big_img': np.arange(9, dtype=np.float32).reshape(3, 3)}
    inputs = {
      'desire_pulse': np.zeros(8, dtype=np.float32),
      'traffic_convention': np.array([1, 0], dtype=np.float32),
      'action_t': np.array([0.05, 0.05], dtype=np.float32),
    }
    values = []
    try:
      for iteration, desire in enumerate([1, 1, 0, 1]):
        inputs['desire_pulse'][1] = desire
        result = model.run(frames, transforms, inputs, prepare_only=iteration == 1)
        values.append(float(result['raw_pred'][0]))
      layout = {
        name: {'shape': list(view.shape), 'offset': view.ctypes.data - model.views['tfm'].ctypes.data, 'dtype': str(view.dtype)}
        for name, view in model.views.items()
      }
    finally:
      model.close()
      original.subprocess.Popen = popen
    events = [json.loads(line) for line in log.read_text().splitlines()]
    expected_desire = [1, 0, 0, 1]
    assert [event['desire'][1] for event in events] == expected_desire
    assert values == [1, 2, 3, 4]
    assert all(event['inputs']['img'] == hashlib.sha256(frames['img'].data.tobytes()).hexdigest() for event in events)
    assert all(event['inputs']['big_tfm'] == hashlib.sha256(transforms['big_img'].tobytes()).hexdigest() for event in events)
    assert process.poll() is not None and not shared_path.exists()
    rows.append(
      {
        'camera': [width, height],
        'invocations': invocations,
        'desire_edges': expected_desire,
        'prepare_only_published': values[1] == 2,
        'values': values,
        'layout': layout,
        'worker_reaped': process.poll() is not None,
        'shared_file_removed': not shared_path.exists(),
      }
    )
  result = {
    'passed': True,
    'rows': rows,
    'scope': 'Unchanged source client against native shared-file/protocol implementation with owned deterministic runtime; not GPU numeric acceptance.',
  }
  (args.evidence / 'comparison.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
