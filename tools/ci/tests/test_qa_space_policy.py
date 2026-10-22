"""#509: actual QA admission boundaries, with controlled disk observations only in tests."""
from __future__ import annotations

import ast
from contextlib import redirect_stderr
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
TOOLS = ROOT / 'rust/tools'
BASELINE = '82e1284d8602120f1e5aa834ea4cea0d2a41fb42'
# Immutable original Git blobs, and independently specified byte-growth inputs.
CASES = {
  "build_athena_vision_peer": {"blob":"c8de481bfd00a06514d261d2edc5796336cba21a","growths":[1073741824]},
  "build_locationd_oracle": {"blob":"980a243ad5d06c1c85c3e0d594cd8076a3fcc3c8","growths":[1073741824]},
  "build_pandad_peripheral_source": {"blob":"9787d647f47d1a06bfe08046da8608f6df4e4ba7","growths":[104857600]},
  "build_paramsd_oracle": {"blob":"df29b40bfcb9c2315b40f1d08db430ff6b0dcdc6","growths":[1073741824,1073741824]},
  "build_plannerd_acados": {"blob":"7657de87988b776809dd86ce4defaac6467b6062","growths":[134217728]},
  "carrot_man_stage_geos": {"blob":"dd9a50f74a5ad7a19ad48682fe55b84e1b3916ec","growths":[1048581]},
  "carrot_recovery_http_control": {"blob":"4ffd07612d1cd0e5847bc610500756249b040eec","growths":[26214437]},
  "check_camerad_camera_lifecycle": {"blob":"1d1df7ff42c948731a0d5ea54be27f49a0520f06","growths":[1073741824]},
  "check_card_factory": {"blob":"e8116577be79290cf2730b49b8de4d0579917e8b","growths":[16777216]},
  "check_cweb_address": {"blob":"301dfd4c41bafd3ac138924ed61bf03b9838b2f8","growths":[1073741824]},
  "check_encoder_runtime": {"blob":"e67d42f9876d1f39a5b6654b3bd8c4180a718491","growths":[1073741824]},
  "check_encoder_v4l": {"blob":"3eddf8d4ebe3f414c6c1cf0712e081d9450258c4","growths":[134217728]},
  "check_jpeg_sanitizers": {"blob":"a721e65d558dc25ac80beef4a60954f38c56f809","growths":[1073741824]},
  "check_locationd_native": {"blob":"f337b0a7f063b290684d8b47673d9c8e80848818","growths":[2147483648]},
  "check_paramsd_native": {"blob":"a1e7580517a25338d14e496cc547b6857c765c23","growths":[2147483648]},
  "check_plannerd_ci": {"blob":"9a110182c9816356197bed74248423b20952ae6b","growths":[536870912]},
  "check_plannerd_lifecycle": {"blob":"3ffbbf628701c81dd1a3a752225f0527906893c0","growths":[134217728]},
  "check_radarcan_constructors": {"blob":"d8b7b77c0ac1a20fc139cbc982937a59c649388d","growths":[536870912]},
  "check_radarcan_runtime": {"blob":"15067032bfb54e5c7214b131178f90d241eac9c5","growths":[268435456]},
  "check_runtime_launcher": {"blob":"a18592a9b8a9846777e9cdfedad5b4d7f72dde15","growths":[536870912]},
  "check_ui_framework": {"blob":"093087bcc5f2f8704b62b7f1cb696134aa3bc4c2","growths":[1073741824]},
  "check_ui_native_safety": {"blob":"7026cc048cf5e7cc7119cd72494368aa4c5cc407","growths":[268435456]},
  "check_ui_root": {"blob":"30f8ff3ea5d5f7ae3b8ddbacb99691fe5ed73724","growths":[536870912]},
  "check_ui_augmented": {"blob":"730743026421958ce3071689ada151d4871a3e11","growths":[536870912]},
  "check_usbgpu_hardware": {"blob":"8ef3e71973018b586bac96d8638b333991aabfc1","growths":[1073741824,1073741824]},
  "check_xiaoge_ci": {"blob":"3b35484ba51f488e81eb5e16e92ad8036313570f","growths":[536870912]},
  "check_xiaoge_inference": {"blob":"9462b5e1bf815715e4f29882184ee85b99328de9","growths":[268435456]},
  "check_xiaoge_lane": {"blob":"be70ad0c16e3fb7f3711c97b919f4362b7d40985","growths":[67108864]},
  "check_xiaoge_lifecycle": {"blob":"9e60b452e12c4e58fbb75e05ce6303e2c54152df","growths":[134217728]},
  "check_xiaoge_live": {"blob":"82127861068f1f87a4a7ad64326af9deda459416","growths":[134217728]},
  "check_xiaoge_native_memory": {"blob":"f3e673ad6723286c02b04fc83624cffd1b3af688","growths":[2147483648]},
  "check_xiaoge_runtime": {"blob":"8dac92258de7154d723c68da9185de8e7d15558d","growths":[134217728]},
  "extract_runtime_sdk": {"blob":"9e993a161d09c6698ecbb222146f6d53c5a4d284","growths":[2147483648]},
  "generate_xiaoge_opencv_reference": {"blob":"46030a6e85bec5a60c13f7b78b6a163829afc99c","growths":[268435456]},
  "radarcan_prepare_assets": {"blob":"36c55bff3383acedcd1d75d4247cbdd3460c9e99","growths":[37]},
  "stage_radarcan_runtime": {"blob":"484462bbf342b4a4d8802ad9131c76efa198a3df","growths":[67108864]},
  "usbgpu_status_fixture": {"blob":"d82b74ead7fed14e7e407eca944a180d33c0faab","growths":[134217728]},
  "webcam_camera_compare": {"blob":"43805a975ddff54ee6e81903ce1e52723fda2132","growths":[536870912]},
  "webcam_capture_errors": {"blob":"1c08750af5dab9a67f3a5169b5333c2df8d74445","growths":[536870912]},
  "webcam_cli_compare": {"blob":"828770183bef9399892f23fcbafde50d97e30454","growths":[536870912]},
  "webcam_worker_compare": {"blob":"a34ca25adb97da9ecac4b047b81c110675aff476","growths":[536870912]},
}


def load_policy():
  spec = importlib.util.spec_from_file_location('qa_space_policy_509', TOOLS / 'qa_space_policy.py')
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  return module


def original_source(name, identity):
  data = subprocess.check_output(['git', 'cat-file', 'blob', identity], cwd=ROOT)
  actual = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
  if actual != identity:
    raise AssertionError(f'original Git blob differs: {name}')
  selected = subprocess.check_output(['git', 'rev-parse', f'{BASELINE}:rust/tools/{name}.py'], cwd=ROOT, text=True).strip()
  if selected != identity:
    raise AssertionError(f'original path identity differs: {name}')
  return data.decode()


def capacity_blocks(source):
  """Select actual admission statements, never import/execute the QA module."""
  tree = ast.parse(source)
  blocks = []
  for parent in ast.walk(tree):
    for field in ('body', 'orelse', 'finalbody'):
      body = getattr(parent, field, ())
      if not isinstance(body, list):
        continue
      for index, statement in enumerate(body):
        probe = statement.value if isinstance(statement, (ast.Assign, ast.AnnAssign)) else (
          statement.test if isinstance(statement, (ast.If, ast.Assert)) else None)
        if probe is None:
          continue
        measured = any(isinstance(node, ast.Call) and (
          isinstance(node.func, ast.Name) and node.func.id == 'free_for_growth' or
          isinstance(node.func, ast.Attribute) and node.func.attr == 'disk_usage')
          for node in ast.walk(probe))
        if not measured:
          continue
        block = []
        # Retain an adjacent literal growth declaration (e.g. radar staging).
        # Metadata-derived estimates outside the guard are controlled inputs.
        if index:
          previous = body[index - 1]
          if (isinstance(previous, ast.Assign) and len(previous.targets) == 1 and
              isinstance(previous.targets[0], ast.Name) and previous.targets[0].id == 'growth' and
              all(isinstance(node, (ast.Constant, ast.BinOp, ast.UnaryOp, ast.operator, ast.unaryop))
                  for node in ast.walk(previous.value))):
            block.append(previous)
        block.append(statement)
        if not isinstance(statement, (ast.If, ast.Assert)):
          for following in body[index + 1:]:
            block.append(following)
            if isinstance(following, (ast.If, ast.Assert)):
              break
          else:
            raise AssertionError('disk observation has no admission decision')
        blocks.append(block)
  return tree, sorted(blocks, key=lambda block: block[0].lineno)


def run_block(policy, source, index, free, directory, *, reserve_mib=512):
  tree, blocks = capacity_blocks(source)
  arguments = SimpleNamespace(output=directory, reserve_mib=reserve_mib)
  namespace = {'shutil': shutil, 'json': json, 'free_for_growth': policy.free_for_growth,
               'ROOT': directory, 'root': directory, 'path': directory, 'output': directory,
               'destination': directory / 'assets', 'args': arguments, 'arguments': arguments,
               'growth': 37, 'files': {'owned': b'abcde'}, 'index': 0}
  # Preserve the real custom admission exception and its constructor.
  classes = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'InsufficientSpace']
  if classes:
    exec(compile(ast.Module(body=classes, type_ignores=[]), '<actual admission exception>', 'exec'), namespace)
  program = compile(ast.Module(body=blocks[index], type_ignores=[]), '<actual QA admission>', 'exec')
  stdout = io.StringIO()
  failure = None
  with patch.object(shutil, 'disk_usage', return_value=SimpleNamespace(total=free, used=0, free=free)), redirect_stderr(stdout):
    try:
      exec(program, namespace)
    except (AssertionError, OSError, RuntimeError, ValueError) as error:
      failure = error
  receipts = [json.loads(line) for line in stdout.getvalue().splitlines() if line]
  return failure, receipts


class QaSpacePolicy509Tests(unittest.TestCase):
  def test_actual_registered_guards_admit_exact_growth_and_reject_one_byte_below(self):
    # Reintroducing a reserve, deleting rejection, or using > instead of >= fails.
    policy = load_policy()
    with TemporaryDirectory() as temporary:
      directory = Path(temporary)
      for name, case in CASES.items():
        source = (TOOLS / f'{name}.py').read_text()
        original = original_source(name, case['blob'])
        self.assertEqual(len(capacity_blocks(source)[1]), len(case['growths']), name)
        self.assertEqual(len(capacity_blocks(original)[1]), len(case['growths']), name)
        for index, growth in enumerate(case['growths']):
          with self.subTest(name=name, index=index, growth=growth):
            error, receipts = run_block(policy, source, index, growth, directory)
            self.assertIsNone(error, f'{name}: {error}')
            self.assertEqual(len(receipts), 1)
            self.assertEqual(receipts[0]['free_bytes'], growth)
            self.assertEqual(receipts[0]['growth_bytes'], growth)
            self.assertEqual(receipts[0]['required_bytes'], growth)
            self.assertEqual(receipts[0]['reserve_bytes'], 0)
            self.assertTrue(receipts[0]['admitted'])
            if name == 'check_encoder_v4l':
              saved = json.loads((directory / 'build-0-space.json').read_text())
              self.assertEqual(saved, {'free': growth, 'growth': growth, 'floor': 0})
            error, receipts = run_block(policy, source, index, growth - 1, directory)
            self.assertIsNotNone(error, name)
            self.assertFalse(receipts[0]['admitted'])
            old_error, _ = run_block(policy, original, index, growth - 1, directory)
            self.assertEqual(type(error).__name__, type(old_error).__name__, name)

  def test_authenticated_original_guards_reject_the_same_exact_growth(self):
    # This executes unchanged original admission ASTs, not reconstructed guards.
    policy = load_policy()
    with TemporaryDirectory() as temporary:
      for name, case in CASES.items():
        source = original_source(name, case['blob'])
        for index, growth in enumerate(case['growths']):
          with self.subTest(name=name, index=index):
            error, receipts = run_block(policy, source, index, growth, Path(temporary))
            self.assertIsNotNone(error, 'original retired reserve unexpectedly admitted exact growth')
            self.assertEqual(receipts, [])

  def test_actual_parameterized_webcam_guards_accept_zero_and_large_growth(self):
    policy = load_policy()
    with TemporaryDirectory() as temporary:
      for name in ('webcam_camera_compare', 'webcam_capture_errors', 'webcam_cli_compare', 'webcam_worker_compare'):
        source = (TOOLS / f'{name}.py').read_text()
        for mib in (0, 1, 1 << 60):
          with self.subTest(name=name, mib=mib):
            growth = mib * 1048576
            error, receipts = run_block(policy, source, 0, growth, Path(temporary), reserve_mib=mib)
            self.assertIsNone(error)
            self.assertEqual(receipts[0]['required_bytes'], growth)
            if growth:
              error, receipts = run_block(policy, source, 0, growth - 1, Path(temporary), reserve_mib=mib)
              self.assertIsNotNone(error)
              self.assertFalse(receipts[0]['admitted'])

  def test_policy_keeps_exact_zero_and_large_integer_observations_without_wrap(self):
    policy = load_policy()
    for growth in (0, 1, 16777216, 268435456, (1 << 64) - 1, (1 << 128) - 1):
      for free in {growth, growth + 1, max(0, growth - 1)}:
        with self.subTest(free=free, growth=growth), patch.object(policy.shutil, 'disk_usage', return_value=SimpleNamespace(free=free)):
          stdout = io.StringIO()
          with redirect_stderr(stdout):
            observed = policy.free_for_growth(Path('/owned/path with spaces'), growth)
          self.assertEqual(observed, free)
          receipt = json.loads(stdout.getvalue())
          self.assertEqual(receipt['free_bytes'], free)
          self.assertEqual(receipt['growth_bytes'], growth)
          self.assertEqual(receipt['required_bytes'], growth)
          self.assertEqual(receipt['reserve_bytes'], 0)
          self.assertEqual(receipt['admitted'], free >= growth)

  def test_negative_growth_is_rejected_and_metadata_errors_propagate(self):
    policy = load_policy()
    with TemporaryDirectory() as temporary:
      with self.assertRaisesRegex(ValueError, 'nonnegative'):
        policy.free_for_growth(Path(temporary), -1)
      with self.assertRaises(FileNotFoundError):
        policy.free_for_growth(Path(temporary) / 'missing', 0)
      stdout = io.StringIO()
      with redirect_stderr(stdout):
        observed = policy.free_for_growth(Path(temporary), 0)
      receipt = json.loads(stdout.getvalue())
      self.assertEqual(observed, receipt['free_bytes'])
      self.assertTrue(receipt['admitted'])
      self.assertEqual(receipt['reserve_bytes'], 0)

  def test_safe_path_children_import_the_policy_using_actual_registered_job_paths(self):
    workflow = (ROOT / '.github/workflows/rust.yml').read_text()
    jobs = dict(re.findall(r'^  ([\w-]+):\n(.*?)(?=^  [\w-]+:|\Z)', workflow, re.M | re.S))
    # These jobs launch affected -P scripts directly or through their QA driver.
    for job in ('workspace', 'webcam-runtime', 'planner-runtime', 'encoder-runtime',
                'radar-runtime', 'radar-arm', 'xiaoge-runtime'):
      value = re.search(r'^      PYTHONPATH: (.+)$', jobs[job], re.M).group(1)
      python_path = value.replace('${{ github.workspace }}', str(ROOT))
      self.assertNotIn('${{', python_path)
      for cwd in (ROOT, ROOT / 'rust'):
        with self.subTest(job=job, cwd=cwd):
          environment = os.environ | {'PYTHONPATH': python_path}
          result = subprocess.run([sys.executable, '-P', '-c',
                                   'import qa_space_policy; print(qa_space_policy.__file__)'],
                                  cwd=cwd, env=environment, text=True, capture_output=True, timeout=5)
          self.assertEqual(result.returncode, 0, result.stderr)
          self.assertEqual(Path(result.stdout.strip()).resolve(), (TOOLS / 'qa_space_policy.py').resolve())


if __name__ == '__main__':
  unittest.main()
