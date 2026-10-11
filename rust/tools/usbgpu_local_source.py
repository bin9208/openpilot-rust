from __future__ import annotations

import json
from pathlib import Path

import numpy as np
from tinygrad import Device, Tensor, TinyJit

from model_export.amd import export_amd
from model_export.original import driving
from openpilot.selfdrive.modeld.helpers import dump_oob
from openpilot.system.camerad.cameras.nv12_info import get_nv12_info
from openpilot.selfdrive.modeld.compile_modeld import NV12Frame, POLICY_INPUTS, WARP_DEV, make_input_queues, make_run_policy, make_warp


def owned_null_storage(device: str | None = None) -> None:
  allocator = Device[device or Device.DEFAULT].allocator
  allocator._alloc = lambda size, _options: memoryview(bytearray(size))
  allocator._offset = lambda buffer, size, offset: buffer[offset : offset + size]
  allocator._copyin = lambda destination, source: destination.cast('B').__setitem__(slice(None), source.cast('B'))
  allocator._copyout = lambda destination, source: destination.cast('B').__setitem__(slice(None), source.cast('B'))


def model_runner(inputs: dict[str, Tensor]) -> dict[str, Tensor]:
  images = inputs['img'].cast('float32').reshape(-1)[:4]
  big_images = inputs['big_img'].cast('float32').reshape(-1)[:4]
  features = inputs['features_buffer'].sum(axis=(0, 1))
  desire = inputs['desire_pulse'].sum()
  scalars = inputs['traffic_convention'].sum() + inputs['action_t'].sum()
  hidden = features + images + big_images + desire + scalars
  return {'output': Tensor.cat(images, hidden).reshape(1, 8)}


def run(destination: Path, *, capture: bool, camera: tuple[int, int] = (1344, 760), export_graph: bool = True) -> None:
  if capture:
    owned_null_storage()
    if WARP_DEV:
      owned_null_storage(WARP_DEV)
  width, height = camera
  stride, y_height, uv_height, frame_size = get_nv12_info(width, height)
  metadata = {
    'model_checkpoint': 'owned-small-run-policy',
    'input_shapes': {
      'img': (1, 12, 2, 2),
      'big_img': (1, 12, 2, 2),
      'features_buffer': (1, 2, 4),
      'desire_pulse': (1, 3, 8),
      'traffic_convention': (1, 2),
      'action_t': (1, 2),
    },
    'output_shapes': {'output': (1, 8)},
    'output_slices': {'policy': slice(0, 4), 'hidden_state': slice(4, 8)},
  }
  queues, numpy_inputs = make_input_queues(metadata['input_shapes'], 4, Device.DEFAULT)
  image = np.arange(frame_size, dtype=np.uint32).astype(np.uint8)
  warp = TinyJit(make_warp(NV12Frame(width, height, stride, y_height, uv_height, frame_size), 4, 4, 4), prune=True)
  warp_inputs = {
    'frame': Tensor(image.copy(), device=WARP_DEV).realize(),
    'big_frame': Tensor((image + 1).astype(np.uint8), device=WARP_DEV).realize(),
    'tfm': queues['tfm'],
    'big_tfm': queues['big_tfm'],
  }
  warped = warp(**warp_inputs)
  warped = warp(**warp_inputs)
  policy = TinyJit(make_run_policy(model_runner, metadata, 4), prune=True)
  policy_inputs = {key: queues[key] for key in POLICY_INPUTS} | {'warped': warped}
  policy(**policy_inputs)
  output = policy(**policy_inputs)[0]
  outputs = {'model': output, 'warped': warped, **{name: queues[name] for name in ('img_q', 'big_img_q', 'feat_q', 'desire_q')}}
  if capture:
    artifact = destination / 'small-driving.pkl'
    with artifact.open('wb') as source:
      dump_oob({(width, height): warp, 'run_policy': policy, 'metadata': metadata}, source)
    if export_graph:
      original = driving(artifact, (width, height))
      export_amd(original.pipeline, original.bindings, destination / 'bundle')
    return
  for name in ('img_q', 'big_img_q', 'feat_q', 'desire_q'):
    queues[name].assign(Tensor.zeros(queues[name].shape, dtype=queues[name].dtype)).realize()
  sequence = []
  for frame in range(15):
    matrix = np.array([[1, 0, frame % 3], [0, 1, frame % 2], [0, 0, 1]], dtype=np.float32)
    numpy_inputs['tfm'][:] = numpy_inputs['big_tfm'][:] = matrix
    numpy_inputs['desire'][:] = 0
    numpy_inputs['desire'][frame % 8] = 1
    numpy_inputs['traffic_convention'][:] = [1, 0]
    numpy_inputs['action_t'][:] = [frame % 3, 0]
    prepare_inputs = {}
    for name, delta in (('frame', frame), ('big_frame', 2 * frame + 1)):
      values = (image + delta).astype(np.uint8)
      warp_inputs[name].assign(Tensor(values)).realize()
      filename = f'{name}-{frame}.bin'
      (destination / filename).write_bytes(values.tobytes())
      prepare_inputs[name] = filename
    for name in ('tfm', 'big_tfm'):
      filename = f'{name}-{frame}.bin'
      (destination / filename).write_bytes(numpy_inputs[name].tobytes())
      prepare_inputs[name] = filename
    warp(**warp_inputs)
    before = {}
    for name in outputs:
      if frame == 0 and name == 'model':
        continue
      filename = f'native-prepare-{name}-{frame}.bin'
      (destination / f'source-prepare-{name}-{frame}.bin').write_bytes(outputs[name].numpy().tobytes())
      before[name] = filename
    sequence.append({'entrypoint': 'prepare', 'inputs': prepare_inputs, 'outputs': before})
    if frame in (1, 7, 12):
      continue
    filename = f'packed-{frame}.bin'
    (destination / filename).write_bytes(queues['packed_npy_inputs'].numpy().tobytes())
    output = policy(**policy_inputs)[0]
    numpy_inputs['prev_feat'][:] = output.numpy()[0, 4:8]
    expected = {}
    for name in outputs:
      filename = f'native-policy-{name}-{frame}.bin'
      (destination / f'source-policy-{name}-{frame}.bin').write_bytes(outputs[name].numpy().tobytes())
      expected[name] = filename
    sequence.append({'entrypoint': 'policy', 'inputs': {'packed_npy_inputs': f'packed-{frame}.bin'}, 'outputs': expected})
  (destination / 'sequence.json').write_text(json.dumps(sequence) + '\n')
