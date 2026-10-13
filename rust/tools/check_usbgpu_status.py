# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# How to run:
# python rust/tools/check_usbgpu_status.py --binary BIN --evidence FILE [--fresh-firmware]
"""Unchanged source comparisons and explicit native package-admission controls."""

from __future__ import annotations
import argparse
import ast
from dataclasses import asdict
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import types
from typing import TypedDict

from usbgpu_status_fixture import Case, Fixture, create, hardware, mutate, sha256

ROOT = pathlib.Path(__file__).resolve().parents[2]


class Status(TypedDict):
  compiled: bool
  compile_pending: bool


class NativeRun(TypedDict):
  invocation: list[str]
  input_paths: list[str]
  returncode: int
  stdout: str
  stderr: str
  status: Status


def run_native(binary: str, paths: list[str]) -> NativeRun:
  process = subprocess.run([binary], input=json.dumps(paths) + '\n', text=True, capture_output=True, timeout=5)
  assert process.returncode == 0, process.stderr
  value = json.loads(process.stdout)
  assert set(value) == {'compiled', 'compile_pending'} and all(type(flag) is bool for flag in value.values()), value
  return {'invocation': [binary], 'input_paths': paths, 'returncode': process.returncode,
          'stdout': process.stdout, 'stderr': process.stderr,
          'status': {'compiled': value['compiled'], 'compile_pending': value['compile_pending']}}


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
  parser.add_argument('--fresh-firmware', action='store_true', help='fetch pinned firmware into a fresh owned cache, ignoring local packages')
  args = parser.parse_args()
  base = ROOT / 'openpilot/selfdrive/modeld'
  big = source('big_model_status_oracle', base / 'big_model.py')
  pre = source('openpilot.selfdrive.modeld.precompiled_model', base / 'precompiled_model.py')
  helper = source('helper_status_oracle', base / 'helpers.py', {
    'modeld_pkl_path', 'active_usbgpu_compiled_path', 'usbgpu_compile_pending', 'usbgpu_compiled_path', 'usbgpu_compiled',
  })
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
      observable = run_native(args.binary, [str(models), str(cache)])
      actual = observable['status']
      active_valid = not previous and corrupt not in {'state', 'size'}
      source_installed = installed and not rejected and corrupt not in {'runtime', 'catalog', 'modelhash'}
      source_expected = {'compiled': active_valid and (source_installed or local and filename.endswith('.onnx')),
                         'compile_pending': active_valid and not source_installed and not local}
      native_expected = {'compiled': False, 'compile_pending': active_valid}
      results.append(
        {
          'scenario': {'filename': filename, 'local': local, 'installed': installed, 'rejected': rejected, 'corrupt': corrupt, 'previous': previous},
          'source': expected,
          'native': actual,
          'passed': expected == actual,
          'native_expected': native_expected,
          'source_expected': source_expected,
          'policy_passed': actual == native_expected and expected == source_expected,
          'classification': 'source_parity' if expected == actual else 'strict_native_admission_rejects_legacy_placeholder',
          'observable': observable,
        }
      )
  args.evidence.parent.mkdir(parents=True, exist_ok=True)
  native_results = []
  cases: list[tuple[Case, Status, Status]] = [
    ('chunked', {'compiled': True, 'compile_pending': False}, {'compiled': True, 'compile_pending': False}),
    ('flat', {'compiled': True, 'compile_pending': False}, {'compiled': False, 'compile_pending': True}),
    ('previous', {'compiled': True, 'compile_pending': False}, {'compiled': True, 'compile_pending': False}),
    ('precompiled_selection', {'compiled': False, 'compile_pending': True}, {'compiled': False, 'compile_pending': False}),
    *[(case, {'compiled': False, 'compile_pending': True}, {'compiled': True, 'compile_pending': False}) for case in (
      'missing_package', 'missing_pointer', 'bad_pointer', 'pointer_identity', 'missing_camera', 'selected_identity',
      'source_hash', 'missing_chunk', 'chunk_count', 'manifest_hash', 'kernel_hash', 'weights_hash', 'graph_layout',
      'unlisted_graph_file', 'firmware_pin', 'symlink', 'rejected',
    )],
    ('previous_rejected', {'compiled': False, 'compile_pending': False}, {'compiled': True, 'compile_pending': False}),
    ('bad_state', {'compiled': False, 'compile_pending': False}, {'compiled': False, 'compile_pending': False}),
    ('missing_original', {'compiled': False, 'compile_pending': False}, {'compiled': False, 'compile_pending': False}),
  ]
  with tempfile.TemporaryDirectory(prefix='status-admission-', dir=args.evidence.parent) as temp:
    root = pathlib.Path(temp)
    firmware_receipts = hardware(root / 'firmware', fresh=args.fresh_firmware)
    baseline = create(root / 'baseline', root / 'firmware')
    for case, native_expected, source_expected in cases:
      fixture = Fixture(root / case, baseline.model_sha256, baseline.pickle_sha256)
      shutil.copytree(baseline.root, fixture.root)
      mutate(fixture, case)
      big.model_cache_dir = lambda cache=fixture.cache: cache
      helper.active_manifest = big.active_manifest
      helper.read_state = big.read_state
      helper.MODELS_DIR = fixture.models
      pre.active_manifest = big.active_manifest
      pre.model_cache_dir = lambda cache=fixture.cache: cache
      observed_source = {'compiled': helper.usbgpu_compiled(), 'compile_pending': helper.usbgpu_compile_pending()}
      observable = run_native(args.binary, fixture.input_paths())
      native_results.append({'scenario': case, 'source': observed_source, 'source_expected': source_expected,
                             'native': observable['status'], 'native_expected': native_expected, 'observable': observable,
                             'passed': observed_source == source_expected and observable['status'] == native_expected})
    fixture_identity = asdict(baseline)
    fixture_identity['root'] = str(fixture_identity['root'])
  divergences = sum(not result['passed'] for result in results)
  document = {
    'invocation': sys.argv, 'binary_sha256': sha256(pathlib.Path(args.binary).read_bytes()),
    'source_sha256': {name: sha256((base / name).read_bytes()) for name in ['big_model.py', 'precompiled_model.py', 'helpers.py']},
    'scope': {'legacy_source': 'original active_usbgpu_compiled_path and usbgpu_compile_pending function bodies',
              'native_fixture_source': 'original usbgpu_compiled and usbgpu_compile_pending function bodies, including previous fallback',
              'native_fixture': 'synthetic PKL, model graph and probe; genuine pinned firmware and license; structural admission only, no GPU execution',
              'policy': 'native status requires a validated local or installed native package; ' +
                        'Python entry placeholders and presence-only chunk markers do not qualify'},
    'summary': {'legacy_cases': len(results), 'legacy_parity': len(results) - divergences, 'strict_policy_divergences': divergences,
                'native_admission_cases': len(native_results)},
    'results': results, 'native_admission': native_results, 'fixture_identity': fixture_identity,
    'firmware': firmware_receipts, 'fresh_firmware': args.fresh_firmware,
  }
  args.evidence.write_text(json.dumps(document, indent=2) + '\n')
  assert len(results) == 192 and divergences == 34, document['summary']
  failures = [v for v in results if not v['policy_passed']] + [v for v in native_results if not v['passed']]
  assert not failures, failures[:5]
  print(f'PASS {len(results)} unchanged-source cases ({len(results) - divergences} parity, {divergences} strict-policy divergences);',
        f'{len(native_results)} native admission controls')


if __name__ == '__main__':
  main()
