# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3"]
# ///
# Run with the locked UI oracle Python: PYTHONPATH=. python rust/tools/check_ui_model_geometry.py BINARY OUTPUT
"""Compare all native geometry outputs with unchanged source helpers."""

from dataclasses import asdict
import ast
import json
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace
from typing import assert_never
import numpy as np
from ui_model_qa.geometry_cases import Case, cases
from openpilot.selfdrive.ui.onroad.path_geometry import sample_path, project_path
from openpilot.selfdrive.ui.road_markings import lane_dash_segments, project_lane_segments, project_blindspot_barrier, blindspot_barrier_quads


def helper(path: Path):
  tree = ast.parse(path.read_text())
  selected = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'ModelRenderer')
  selected.bases = []
  selected.body = [node for node in selected.body if isinstance(node, ast.FunctionDef) and node.name in ('_map_line_to_polygon', '_map_to_screen')]
  namespace = {'np': np}
  exec(compile(ast.Module(body=[selected], type_ignores=[]), str(path), 'exec'), namespace)
  return namespace['ModelRenderer']


def source(case: Case):
  line = np.asarray(case.line, dtype=np.float32).reshape(-1, 3)
  matrix = np.asarray(case.transform, dtype=np.float32)
  clip = SimpleNamespace(**dict(zip(('x', 'y', 'width', 'height'), case.clip, strict=True)))
  match case.kind:
    case 'sample':
      value = sample_path(line, case.distances)
    case 'path':
      value = project_path(line, case.width, case.z_start, case.z_end, matrix, clip, case.invert)
    case 'lanes':
      value = project_lane_segments(lane_dash_segments(line, case.distance), case.width, matrix, clip)
      return [points.tolist() for points in value]
    case 'blindspot' | 'quads':
      value = project_blindspot_barrier(line, case.shift, matrix, clip)
      if case.kind == 'quads':
        value = blindspot_barrier_quads(value)
    case 'big_ribbon' | 'small_ribbon' | 'point':
      directory = 'mici/onroad' if case.kind == 'small_ribbon' else 'onroad'
      model = helper(Path('openpilot/selfdrive/ui') / directory / 'model_renderer.py')()
      model._car_space_transform, model._clip_region = matrix, clip
      if case.kind == 'point':
        return model._map_to_screen(*[float(x) for x in (line[0] if len(line) else [0.0, 0.0, 0.0])])
      if case.kind == 'big_ribbon':
        value = model._map_line_to_polygon(line, case.width, case.z_start, case.end, case.distance, case.invert, case.shift)
      else:
        value = model._map_line_to_polygon(line, case.width, case.z_start, case.end, case.invert)
    case unreachable:
      assert_never(unreachable)
  return value.tolist()


def main() -> None:
  binary, output = Path(sys.argv[1]), Path(sys.argv[2])
  output.mkdir(parents=True, exist_ok=True)
  inputs = cases()
  request = [asdict(case) for case in inputs]
  expected = [source(case) for case in inputs]
  (output / 'input.json').write_text(json.dumps(request) + '\n')
  (output / 'source.json').write_text(json.dumps(expected) + '\n')
  native = subprocess.run([binary], input=json.dumps(request), text=True, capture_output=True)
  (output / 'process.log').write_text(native.stderr + f'\nEXIT {native.returncode}\n')
  native.check_returncode()
  (output / 'native.json').write_text(native.stdout)
  actual = json.loads(native.stdout)
  for case, a, b in zip(inputs, expected, actual, strict=True):
    if case.kind in ('sample', 'point'):
      if a is None:
        assert b is None, (case.name, a, b)
      else:
        np.testing.assert_allclose(a, b, rtol=1e-11, atol=1e-11, err_msg=case.name)
    else:
      assert a == b, (case.name, a, b)
  (output / 'results.json').write_text(json.dumps({'pass': True, 'cases': len(inputs), 'float32_exact': True, 'float64_bound': 1e-11}, indent=2) + '\n')


if __name__ == '__main__':
  main()
