"""Input drift and compiler provenance reject before runtime publication."""
import copy
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_runtime_inputs as inputs
from native_model_inputs import InputError, encoded, sha, source_identity


class RuntimeInputsTests(unittest.TestCase):
  def configuration(self):
    return dict(schema=1, compiler_growth_bytes=1,
        providers={role: dict(path='/independent/'+role+'.json', sha256='a'*64) for role in inputs.PROFILES},
        budgets=dict(path='/independent/budgets.json', sha256='b'*64))

  def test_incomplete_provider_selection_never_becomes_a_partial_runtime(self):
    config = self.configuration()
    inputs.validate(config)
    for role in inputs.PROFILES:
      changed = copy.deepcopy(config)
      del changed['providers'][role]
      with self.assertRaises(InputError):
        inputs.validate(changed)
    for path in ('relative.json', '/outside/../source.json', '/double//path.json'):
      changed = copy.deepcopy(config)
      changed['providers']['ffmpeg']['path'] = path
      with self.assertRaises(InputError):
        inputs.validate(changed)

  def test_configuration_bytes_and_duplicate_keys_are_checked_before_any_subprocess(self):
    with tempfile.TemporaryDirectory() as temp, mock.patch.object(inputs.subprocess, 'run') as run:
      path = Path(temp)/'config.json'
      raw = encoded(self.configuration())
      path.write_bytes(raw)
      self.assertEqual(inputs.pinned(path, sha(raw)), self.configuration())
      path.write_bytes(raw+b' ')
      with self.assertRaisesRegex(InputError, 'independent pin'):
        inputs.pinned(path, sha(raw))
      raw = b'{"schema":1,"schema":2}'
      path.write_bytes(raw)
      with self.assertRaisesRegex(InputError, 'duplicate'):
        inputs.pinned(path, sha(raw))
      run.assert_not_called()

  def test_compiler_features_and_target_are_actual_messages_bound_to_output_bytes(self):
    import native_model_generation as generation
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)/'source'
      root.mkdir()
      def git(*args):
        return subprocess.check_output(['/usr/bin/git', '-c', 'user.name=Owned inputs fixture',
            '-c', 'user.email=ci@example.invalid', '-c', 'commit.gpgsign=false',
            '-c', 'core.hooksPath=/dev/null', '-C', root, *args], stderr=subprocess.STDOUT)
      git('init', '-q')
      scope = root/generation.SCOPE_PATH
      scope.parent.mkdir()
      scope.write_bytes(generation.SCOPE_BYTES)
      (root/'rust/Cargo.lock').write_bytes(b'owned lock\n')
      git('add', '.')
      git('commit', '-qm', 'owned compiler identity')
      artifacts, messages = [], []
      for name in ('offline-qcom-compile', 'model-run', 'qcom-model-run'):
        path = Path(temp)/name
        path.write_bytes(b'owned compiler '+name.encode())
        artifacts.append(dict(executable=str(path), bytes=path.stat().st_size, sha256=inputs.digest(path)))
        messages.append(dict(executable=str(path), features=['default', 'native-skip-miri'],
            profile=dict(test=False), target=dict(name=name, kind=['bin'])))
      build = dict(build=dict(artifacts=artifacts, source=source_identity(root)), cargo_artifacts=messages)
      rust = dict(identity='c'*64, tree_sha256='d'*64)
      record, binaries = inputs.compiler_record(root, build, rust, dict(tree_sha256='e'*64), 'f'*64)
      self.assertEqual(record['features'], messages[0]['features'])
      self.assertEqual(record['compiler_sha256'], artifacts[0]['sha256'])
      changed = copy.deepcopy(build)
      changed['build']['source']['commit'] = 'a'*40
      with self.assertRaisesRegex(InputError, 'clean source'):
        inputs.compiler_record(root, changed, rust, dict(tree_sha256='e'*64), 'f'*64)
      for field, bad in (('features', ['default']), ('target', dict(name='other-binary', kind=['bin'])),
                         ('profile', dict(test=True))):
        changed = copy.deepcopy(build)
        changed['cargo_artifacts'][0][field] = bad
        with self.assertRaises(InputError):
          inputs.compiler_record(root, changed, rust, dict(tree_sha256='e'*64), 'f'*64)
      binaries['offline-qcom-compile'].write_bytes(b'damaged output')
      with self.assertRaises(InputError):
        inputs.compiler_record(root, build, rust, dict(tree_sha256='e'*64), 'f'*64)


if __name__ == '__main__':
  unittest.main()
