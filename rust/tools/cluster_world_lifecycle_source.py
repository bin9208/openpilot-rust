# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Execute the unchanged source model loader's missing/reopen lifecycle."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from openpilot.selfdrive.carrot.cluster import cluster_renderer as source


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  original = source.VEHICLE_MODEL_PATH
  missing = args.output / 'missing-source-model.obj'
  assert not missing.exists()
  source.VEHICLE_MODEL_PATH = missing
  renderer = source.ClusterUiRenderer(theme_mode='dark')
  renderer.set_profile_enabled(True)
  try:
    renderer.open(hidden=True)
    assert renderer._vehicle_model is None
    assert renderer._vehicle_model_load_attempted
    source.VEHICLE_MODEL_PATH = original
    renderer._load_vehicle_model()
    assert renderer._vehicle_model is None
    renderer.close()
    assert not renderer._vehicle_model_load_attempted
    renderer.clear_profile_samples()
    renderer.open(hidden=True)
    assert renderer._vehicle_model is not None
    stages = [name for name, _ in renderer.profile_samples() if name.startswith('vehicle_model.')]
    assert stages == ['vehicle_model.parse_obj', 'vehicle_model.upload_mesh', 'vehicle_model.load_from_mesh'], stages
    renderer.close()
    assert renderer._vehicle_model is None
    result = {
      'missing_first': True,
      'same_owner_retry_blocked': True,
      'close_resets_attempt': True,
      'real_model_reloaded': True,
      'stages': stages,
      'closed_model': True,
      'model_sha256': hashlib.sha256(original.read_bytes()).hexdigest(),
      'source_sha256': hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
    }
    (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
  finally:
    renderer.close()
    source.VEHICLE_MODEL_PATH = original


if __name__ == '__main__':
  main()
