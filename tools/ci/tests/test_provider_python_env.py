"""#506 regressions; execution belongs to Actions, including the Bash fragments."""
import ast
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from tools.ci import provider_python_env as receipt


ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = Path(os.environ.get('PROVIDER_PYTHON_WORKFLOW', ROOT / '.github/workflows/rust.yml'))
SOURCE_ROOT = Path(os.environ.get('PROVIDER_PYTHON_SOURCE_ROOT', ROOT))
TAILS = (None, '', '/action/python/lib', '/action/python/lib:/other/lib',
         '/action python/lib:/other directory/lib')


def fragment(source, job):
    # Top-level jobs have two spaces; step/run lines have at least four.
    # Use the complete job below rather than interpreting Bash through YAML.
    lines = source.split(f'  {job}-runtime:\n', 1)[1].splitlines()
    commands = []
    for line in lines:
        if line.startswith('  ') and not line.startswith('   '):
            break
        if line.lstrip().startswith("printf 'OPENPILOT_") and 'LD_LIBRARY_PATH=' in line:
            commands.append(line.strip())
    if len(commands) != 1:
        raise AssertionError((job, commands))
    return commands[0]


def expand(command, directory, tail):
    environment = dict(os.environ, RUNNER_TEMP='/job temp', PYTHONPATH='/existing python:/tail',
                       GITHUB_WORKSPACE='/workspace path', GITHUB_ENV=str(directory / 'env'))
    environment.pop('LD_LIBRARY_PATH', None)
    if tail is not None:
        environment['LD_LIBRARY_PATH'] = tail
    subprocess.run(['bash', '--noprofile', '--norc', '-euc', command], env=environment, check=True)
    return dict(line.split('=', 1) for line in (directory / 'env').read_text().splitlines())


class WorkflowProviderTests(unittest.TestCase):
    def test_actual_printf_preserves_provider_first_and_exact_nonempty_tail(self):
        for job in ('webcam', 'xiaoge'):
            for tail in TAILS:
                with self.subTest(job=job, tail=tail), tempfile.TemporaryDirectory() as temporary:
                    actual = expand(fragment(WORKFLOW.read_text(), job), Path(temporary), tail)
                    provider = '/job temp/' + ('webcam/provider' if job == 'webcam' else 'xiaoge-opencv')
                    self.assertEqual(actual['LD_LIBRARY_PATH'], provider + '/install/lib' + (':' + tail if tail else ''))
                    root_key = 'OPENPILOT_WEBCAM_OPENCV_ROOT' if job == 'webcam' else 'OPENPILOT_OPENCV_ROOT'
                    self.assertEqual(actual[root_key], provider)
                    binding = 'webcam/binding' if job == 'webcam' else 'xiaoge-original/ipc'
                    self.assertEqual(actual['PYTHONPATH'], '/job temp/' + binding + ':/existing python:/tail')
                    if job == 'xiaoge':
                        self.assertEqual(actual['OPENCV_TEST_LANE_MODEL'],
                                         '/workspace path/openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx')

    def test_authenticated_2765_fragments_are_real_erasure_counterexamples(self):
        source = subprocess.check_output(['git', 'show', receipt.BASELINE_COMMIT + ':.github/workflows/rust.yml'],
                                         cwd=ROOT)
        digest = hashlib.sha1(b'blob ' + str(len(source)).encode() + b'\0' + source).hexdigest()
        self.assertEqual(digest, receipt.BASELINE_BLOB)
        for job in ('webcam', 'xiaoge'):
            command = receipt.legacy_fragment(job)
            self.assertEqual(command, fragment(source.decode(), job))
            with self.subTest(job=job), tempfile.TemporaryDirectory() as temporary:
                actual = expand(command, Path(temporary), '/action python/lib:/other/lib')
                provider = '/job temp/' + ('webcam/provider' if job == 'webcam' else 'xiaoge-opencv')
                self.assertEqual(actual['LD_LIBRARY_PATH'], provider + '/install/lib')
                self.assertNotIn('/action python/lib', actual['LD_LIBRARY_PATH'])


def peer_environment(file, function, tail, kind):
    """Execute only actual environment statements, never import the project peer."""
    tree = ast.parse((SOURCE_ROOT / 'rust/tools' / file).read_text())
    definition = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == function)
    assignment = next(node for node in ast.walk(definition) if isinstance(node, ast.Assign)
                      and any(isinstance(target, ast.Name) and target.id == 'environment' for target in node.targets))
    statements = [assignment]
    statements += [node for node in definition.body if isinstance(node, ast.If)
                   and any(isinstance(child, ast.Constant) and child.value == 'LD_LIBRARY_PATH'
                           for child in ast.walk(node))]
    arguments = SimpleNamespace(opencv=Path('/provider path'), peer=kind)
    namespace = dict(os=os, str=str, dict=dict, provider_library_path=receipt.provider_library_path,
                     arguments=arguments, args=arguments, output=Path('/output path'),
                     params=Path('/output path/params'), prefix='owned-prefix', kind=kind)
    environment = {'KEEP': 'unchanged'}
    if tail is not None:
        environment['LD_LIBRARY_PATH'] = tail
    with patch.dict(os.environ, environment, clear=True):
        exec(compile(ast.Module(body=statements, type_ignores=[]), file, 'exec'), namespace)
    return namespace['environment'], definition


class XiaogePeerTests(unittest.TestCase):
    def test_source_peers_and_both_live_python_wrappers_preserve_tail(self):
        for file, function in (('check_xiaoge_runtime.py', 'run_peer'),
                               ('check_xiaoge_lifecycle.py', 'run_peer'),
                               ('check_xiaoge_live.py', 'main')):
            for tail in TAILS:
                for kind in ('source', 'native'):
                    with self.subTest(file=file, tail=tail, kind=kind):
                        actual, _ = peer_environment(file, function, tail, kind)
                        want_tail = tail if kind == 'source' or function == 'main' else None
                        self.assertEqual(actual['LD_LIBRARY_PATH'],
                                         '/provider path/install/lib' + (':' + want_tail if want_tail else ''))
                        self.assertEqual(actual['KEEP'], 'unchanged')
                        self.assertEqual(actual['OPENPILOT_PREFIX'], 'owned-prefix')
                        self.assertEqual(actual['PARAMS_ROOT'], '/output path/params')
                        self.assertEqual(actual['PWD'], '/output path')

    def test_live_native_child_keeps_provider_only_while_source_inherits(self):
        for kind in ('source', 'native'):
            with self.subTest(kind=kind):
                actual, definition = peer_environment('check_xiaoge_live.py', 'peer', '/provider path/install/lib:/action/lib', kind)
                self.assertEqual(actual['LD_LIBRARY_PATH'], '/provider path/install/lib' + (':/action/lib' if kind == 'source' else ''))
                popen = next(node for node in ast.walk(definition) if isinstance(node, ast.Call)
                             and isinstance(node.func, ast.Attribute) and node.func.attr == 'Popen')
                self.assertTrue(any(keyword.arg == 'env' and isinstance(keyword.value, ast.Name)
                                    and keyword.value.id == 'environment' for keyword in popen.keywords))


class ReceiptTests(unittest.TestCase):
    def test_real_stdlib_probe_and_identity_import_failures(self):
        before = receipt.snapshot(('json',))
        self.assertFalse(before['import_errors'])
        self.assertTrue(before['modules']['json']['file'])
        self.assertTrue(before['identity']['executable'])
        self.assertTrue(before['identity']['version'])
        self.assertTrue(before['identity']['prefix'])
        self.assertTrue(before['identity']['base_prefix'])
        self.assertIn('libpython', before['identity'])
        self.assertFalse(receipt.identity_failures(before, receipt.snapshot(('json',))))
        for field in ('executable', 'version', 'prefix', 'base_prefix', 'libpython'):
            altered = dict(before, identity=dict(before['identity'], **{field: 'changed'}))
            self.assertIn(field, receipt.identity_failures(before, altered))
        missing = receipt.snapshot(('json', '_provider_python_506_missing_module'))
        self.assertIn('_provider_python_506_missing_module', missing['import_errors'])
        self.assertIn('imports', receipt.identity_failures(before, missing))
        altered = dict(before, modules={'json': {'file': '/wrong/json.py', 'version': None}})
        self.assertIn('modules', receipt.identity_failures(before, altered))


if __name__ == '__main__':
    unittest.main()
