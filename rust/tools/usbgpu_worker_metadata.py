from __future__ import annotations
import base64
import builtins
import io
import math
import pickle

from usbgpu_hcq_data import ArtifactError


class SliceReader(pickle.Unpickler):
  def find_class(self, module, name):
    if (module, name) == ('builtins', 'slice'):
      return builtins.slice
    raise ArtifactError('unexpected output metadata global')


def export(artifact):
  root = artifact.value
  metadata = root['metadata']['metadata']
  data = base64.b64decode(''.join(metadata['output_slices'].split()), validate=True)
  if len(data) > 65536:
    raise ArtifactError('output metadata size limit')
  stream = io.BytesIO(data)
  slices = SliceReader(stream).load()
  count = math.prod(root['output_specs']['outputs'][0])
  if stream.read(1) or not isinstance(slices, dict):
    raise ArtifactError('invalid output metadata')
  for name, section in slices.items():
    if (
      not isinstance(name, str)
      or not isinstance(section, slice)
      or section.step not in (None, 1)
      or not isinstance(section.start, int)
      or not isinstance(section.stop, int)
      or not 0 <= section.start < section.stop <= count
    ):
      raise ArtifactError('invalid output slice')
  inputs = []
  for name, (shape, dtype, device) in root['input_specs'].items():
    if name == 'new_img' or name.startswith('state_'):
      continue
    if dtype != '<f4' or device != 'AMD' or name not in ('desire', 'traffic_convention', 'action_t'):
      raise ArtifactError('unsupported worker input')
    inputs.append({'name': name, 'shape': list(shape)})
  return {
    'model_sha256': artifact.sha256,
    'checkpoint': metadata['model_checkpoint'],
    'inputs': inputs,
    'output_count': count,
    'output_slices': {name: [value.start, value.stop, value.step] for name, value in slices.items()},
  }
