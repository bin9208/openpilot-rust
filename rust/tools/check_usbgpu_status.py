"""Original filesystem status and explicit missing-native-provider controls."""

from __future__ import annotations
import argparse
import ast
import json
import pathlib
import subprocess
import sys
import tempfile
import types

ROOT = pathlib.Path(__file__).resolve().parents[2]


def source(name, path, selected=None):
  tree = ast.parse(path.read_text())
  tree.body = [
    node
    for node in tree.body
    if not (isinstance(node, ast.ImportFrom) and node.module and node.module.startswith('openpilot.'))
    and (selected is None or isinstance(node, (ast.Import, ast.ImportFrom)) or isinstance(node, ast.FunctionDef) and node.name in selected)
  ]
  module = types.ModuleType(name)
  sys.modules[name] = module
  exec(compile(tree, str(path), 'exec'), module.__dict__)
  return module


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--evidence', type=pathlib.Path, required=True)
  args = parser.parse_args()
  base = ROOT / 'openpilot/selfdrive/modeld'
  big = source('big_model_status_oracle', base / 'big_model.py')
  pre = source('openpilot.selfdrive.modeld.precompiled_model', base / 'precompiled_model.py')
  helper = source('helper_status_oracle', base / 'helpers.py', {'modeld_pkl_path', 'active_usbgpu_compiled_path', 'usbgpu_compile_pending'})
  helper.get_manifest_path = lambda path: str(path) + '.chunkmanifest'
  results = []
  with tempfile.TemporaryDirectory() as temp:
    for index, (filename, local, installed, rejected, corrupt, previous) in enumerate(
      (filename, local, installed, rejected, corrupt, previous)
      for filename in ['model.onnx', 'model.pkl']
      for local in [False, True]
      for installed in [False, True]
      for rejected in [False, True]
      for corrupt in ['none', 'state', 'size', 'runtime', 'catalog', 'modelhash']
      for previous in [False, True]
    ):
      root = pathlib.Path(temp) / str(index)
      cache = root / 'cache'
      models = root / 'models'
      cache.mkdir(parents=True)
      models.mkdir()
      manifest = {'model_id': 'fixture', 'filename': filename, 'size': 3, 'sha256': 'a' * 64, 'url': 'https://models.test/model.onnx'}
      state = {'active': None if previous else manifest, 'previous': manifest if previous else None}
      (cache / 'state.json').write_text('{' if corrupt == 'state' else json.dumps(state))
      (cache / f'model-aaaaaaaaaaaaaaaa{pathlib.Path(filename).suffix}').write_bytes(b'x' if corrupt == 'size' else b'abc')
      if local:
        (models / 'big_driving_aaaaaaaaaaaaaaaa_tinygrad.pkl.chunkmanifest').write_text('1')
      if installed:
        dest = cache / 'precompiled' / ('a' * 64)
        runtime = dest / ('runtime-' + ('b' * 16))
        (runtime / 'tinygrad').mkdir(parents=True)
        (runtime / 'tinygrad/__init__.py').touch()
        generic = filename.endswith('.pkl')
        entry = runtime / ('examples/openpilot/compile_warp.py' if generic else 'model_runtime.py')
        entry.parent.mkdir(parents=True, exist_ok=True)
        if corrupt != 'runtime':
          entry.touch()
        (dest / 'model.pkl').write_bytes(b'abc')
        catalog = {
          'protocol': 1,
          'format': 'comma-generic-onnx' if generic else 'comma-run-model',
          'gpu_arch': 'gfx1200',
          'frame_skip': 4,
          'camera_resolutions': [[1928, 1208], [1344, 760]],
          'catalog_url': 'https://models.test/precompiled.json',
          'runtime_directory': runtime.name,
          'pickle': {'sha256': 'a' * 64, 'size': 3, 'url': 'model.pkl'},
          'runtime': {'sha256': 'b' * 64, 'size': 123, 'url': 'runtime.tar.gz'},
        }
        catalog['model_sha256' if generic else 'onnx_sha256'] = 'c' * 64 if corrupt == 'modelhash' else 'a' * 64
        if corrupt == 'catalog':
          catalog['runtime']['url'] = 'https://other.test/runtime.tar.gz'
        (dest / 'installed.json').write_text(json.dumps(catalog))
        if rejected:
          (dest / 'rejected').touch()
      big.model_cache_dir = lambda cache=cache: cache
      helper.active_manifest = big.active_manifest
      helper.MODELS_DIR = models
      pre.active_manifest = big.active_manifest
      pre.model_cache_dir = lambda cache=cache: cache
      expected = {'compiled': helper.active_usbgpu_compiled_path() is not None, 'compile_pending': helper.usbgpu_compile_pending()}
      assets = root / 'native-assets'
      active = big.active_manifest(cache) is not None
      migration = active and (expected['compiled'] or not expected['compile_pending'])
      native_expected = {'compiled': False, 'compile_pending': True} if migration else expected
      process = subprocess.run([args.binary], input=json.dumps([str(models), str(cache), str(assets)]) + '\n', text=True, capture_output=True, timeout=5)
      assert process.returncode == 0, process.stderr
      actual = json.loads(process.stdout)
      results.append(
        {
          'scenario': {'filename': filename, 'local': local, 'installed': installed, 'rejected': rejected, 'corrupt': corrupt, 'previous': previous},
          'source': expected,
          'native_expected': native_expected,
          'native': actual,
          'provider_migration': migration,
          'passed': native_expected == actual,
        }
      )
  args.evidence.parent.mkdir(parents=True, exist_ok=True)
  args.evidence.write_text(
    json.dumps(
      {
        'invocation': [args.binary],
        'results': results,
        'scope': 'Python runtime files and chunk manifests are source-ready observations; without native assets they cannot select a native worker.',
      },
      indent=2,
    )
    + '\n'
  )
  failures = [v for v in results if not v['passed']]
  assert not failures, failures[:5]
  print(f'PASS {len(results)} missing-native-provider controls; {sum(v["provider_migration"] for v in results)} explicit source differences')


if __name__ == '__main__':
  main()
