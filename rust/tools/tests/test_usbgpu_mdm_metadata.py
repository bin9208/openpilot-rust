"""Pinned MDM metadata must survive the build-time importer unchanged."""
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from usbgpu_worker_metadata import export
from usbgpu_hcq_data import ArtifactError


class MdmMetadata(unittest.TestCase):
  def artifact(self):
    root = json.loads((Path(__file__).parent / 'fixtures/mdm_retrained_metadata.json').read_text())
    return SimpleNamespace(value=root, sha256='256f2178366479a211933701c03518621ed3970289e6a8a43e5a2bcb15e55b5f')

  def test_exact_source_mdm_scalar_contract_and_outputs(self):
    result = export(self.artifact())
    self.assertEqual(result['inputs'], [
      {'name': 'desire', 'shape': [8]},
      {'name': 'traffic_convention', 'shape': [1, 2]},
      {'name': 'action_t', 'shape': [1, 2]},
    ])
    self.assertEqual(result['output_count'], 18452)
    self.assertEqual(result['output_slices']['hidden_state'], [2066, 18450, None])
    self.assertEqual(result['checkpoint'], 'f22e06e5-3b3f-48d2-a6b9-2f32dc20e707/56320/8d736683-fed4-4e48-96fd-eae5d9d5bc56/12864')

  def test_aliases_do_not_accept_other_device_or_precision(self):
    for dtype, device in [('float64', 'AMD'), ('float32', 'CPU')]:
      artifact = self.artifact()
      artifact.value['input_specs']['desire'][1:] = [dtype, device]
      with self.assertRaises(ArtifactError):
        export(artifact)


if __name__ == '__main__':
  unittest.main()
