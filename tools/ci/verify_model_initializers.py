#!/usr/bin/env python3
"""Read both exact source ONNX objects before expensive AGNOS validation."""
import argparse
import codecs
import hashlib
import json
import os
import pickle
from pathlib import Path
import time

import onnx
from onnx import numpy_helper
from tinygrad import dtypes
from tinygrad.nn.onnx import OnnxDataType

from model_export.offline_onnx import load_metadata, load_runner
from native_model_generation import model_inputs
from native_model_inputs import digest, source_identity


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--root', required=True, type=Path)
  parser.add_argument('--work', required=True, type=Path)
  args = parser.parse_args()
  source = args.root.resolve(strict=True)
  origin = source_identity(source)
  assert origin['commit'] == os.environ['GITHUB_SHA']
  assert (args.work/'owner').read_text() == ':'.join(os.environ[k] for k in
      ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
  receipts = json.loads((args.work/'native-model-assets-initial.json').read_text())
  models = model_inputs(source, origin['commit'])
  rows = {}
  for kind, spec in models.items():
    started = time.monotonic()
    path = Path(receipts['model-'+kind]['path']) if spec['lfs'] else source/spec['path']
    assert digest(path, spec['bytes']) == spec['sha256']
    mode = path.stat().st_mode
    if spec['lfs']: assert mode & 0o222 == 0
    proto = onnx.load_model(path, load_external_data=False)
    constants = {tensor.name: tensor for tensor in proto.graph.initializer}
    loaded = load_runner(path, spec['sha256'])
    data = {id(item.buffer): item.data for item in loaded.initialized}
    checked = []
    for name, tensor in constants.items():
      assert not tensor.external_data and tensor.data_location == onnx.TensorProto.DEFAULT
      expected = numpy_helper.to_array(tensor)
      actual = loaded.runner.graph_values[name]
      assert actual.dtype == OnnxDataType(tensor.data_type).to_dtype()
      assert actual.shape == tuple(tensor.dims)
      blob = data[id(actual.uop.base)]
      expected_bytes = expected.tobytes()
      if blob != expected_bytes:
        offsets = []
        for offset, (got, want) in enumerate(zip(blob, expected_bytes)):
          if got != want:
            offsets.append(offset)
            if len(offsets) == 8: break
        mismatch = dict(schema=1, source=origin, model=kind, name=name,
            data_type=tensor.data_type, dtype=str(actual.dtype), shape=list(tensor.dims),
            expected_bytes=len(expected_bytes), actual_bytes=len(blob),
            expected_sha256=hashlib.sha256(expected_bytes).hexdigest(),
            actual_sha256=hashlib.sha256(blob).hexdigest(),
            raw_bytes=len(tensor.raw_data), initialized_buffers=len(loaded.initialized),
            unique_buffers=len(data), first_differing_offsets=offsets,
            samples=[dict(offset=offset, expected=expected_bytes[max(0, offset-4):offset+8].hex(),
                actual=blob[max(0, offset-4):offset+8].hex()) for offset in offsets])
        (args.work/'model-initializer-mismatch.json').write_text(json.dumps(mismatch, indent=2)+'\n')
        print(json.dumps(mismatch), flush=True)
        raise AssertionError(f'initializer bytes differ from original ONNX: {kind}/{name}')
      if actual.dtype == dtypes.bool: assert set(blob) <= {0, 1}
      checked.append(dict(name=name, data_type=tensor.data_type, shape=list(tensor.dims),
          bytes=len(blob), sha256=hashlib.sha256(blob).hexdigest()))
    assert checked and len(checked) == len(data) == len(loaded.initialized)
    properties = {item.key: item.value for item in proto.metadata_props}
    expected_metadata = dict(model_checkpoint=properties.get('model_checkpoint'),
        output_slices=pickle.loads(codecs.decode(properties['output_slices'].encode(), 'base64')),
        input_shapes={item.name: tuple(dim.dim_value if dim.HasField('dim_value') else 0
            for dim in item.type.tensor_type.shape.dim) for item in proto.graph.input},
        output_shapes={item.name: tuple(dim.dim_value if dim.HasField('dim_value') else 0
            for dim in item.type.tensor_type.shape.dim) for item in proto.graph.output})
    metadata = load_metadata(path, spec['sha256'])
    assert metadata == expected_metadata, f'model metadata differs from original ONNX: {kind}'
    assert digest(path, spec['bytes']) == spec['sha256'] and path.stat().st_mode == mode
    rows[kind] = dict(model_sha256=spec['sha256'], bytes=spec['bytes'],
        loaded_initializers=len(data), independently_compared_constants=checked,
        metadata=dict(model_checkpoint=metadata['model_checkpoint'], input_shapes=metadata['input_shapes'],
            output_shapes=metadata['output_shapes'], output_slices={name: dict(start=value.start,
                stop=value.stop, step=value.step) for name, value in metadata['output_slices'].items()}),
        independently_compared_metadata=True,
        elapsed_seconds=time.monotonic()-started)
  assert set(rows) == {'driving', 'driver'} and source_identity(source) == origin
  report = dict(schema=1, source=origin, models=rows, gpu_executed=False,
      full_model_generation=False, full_runtime_build=False, source_checkout_mutated=False,
      scope='Exact source model initializer reads on hosted CPU; full AGNOS generation remains required')
  (args.work/'model-initializer-preflight.json').write_text(json.dumps(report, indent=2)+'\n')
  print(json.dumps({kind:dict(initializers=row['loaded_initializers'],
      compared_constants=len(row['independently_compared_constants']), elapsed_seconds=row['elapsed_seconds'])
      for kind, row in rows.items()}))


if __name__ == '__main__':
  main()
