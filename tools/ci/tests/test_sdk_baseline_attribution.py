"""#292 behavioral RED must not pass because the restored old disk policy fails."""
import ast
import hashlib
from pathlib import Path
import subprocess
from types import SimpleNamespace
import unittest

ROOT = Path(__file__).resolve().parents[3]
BASE = 'b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a'
BLOB = '007cc4f723139d3a15fd986a76fb5102f3dc9131'


def helpers():
    source = ast.parse((ROOT / 'rust/tools/issue_497_baseline.py').read_text())
    names = {'sdk_baseline_with_current_space_policy', 'sdk_alias_failure'}
    selected = [node for node in source.body if isinstance(node, ast.FunctionDef) and node.name in names]
    if {node.name for node in selected} != names:
        raise AssertionError('Missing SDK baseline controls')
    namespace = {}
    exec(compile(ast.Module(body=selected, type_ignores=[]), '<actual baseline helpers>', 'exec'), namespace)
    return namespace


def guard(tree):
    return next(node for node in ast.walk(tree) if isinstance(node, ast.If)
                and 'shutil.disk_usage(args.output).free' in ast.unparse(node.test))


class SdkBaselineAttributionTests(unittest.TestCase):
    def test_authenticated_sdk_walker_is_unchanged_except_current_capacity_policy(self):
        raw = subprocess.check_output(['git', 'show', BASE + ':rust/tools/extract_runtime_sdk.py'], cwd=ROOT)
        self.assertEqual(hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest(), BLOB)
        modified = helpers()['sdk_baseline_with_current_space_policy'](raw)
        original, candidate = ast.parse(raw), ast.parse(modified)
        old_guard, new_guard = guard(original), guard(candidate)
        growth = 2 * 1024**3
        lookup = Path('/owned/sdk path')
        for selected, should_admit in ((old_guard, False), (new_guard, True)):
            for free in (growth - 1, growth):
                calls = []
                def disk_usage(path):
                    calls.append(path)
                    return SimpleNamespace(free=free)
                namespace = {'shutil': SimpleNamespace(disk_usage=disk_usage),
                             'args': SimpleNamespace(output=lookup)}
                program = compile(ast.Module(body=[selected], type_ignores=[]), '<actual SDK guard>', 'exec')
                if free == growth and should_admit:
                    exec(program, namespace)
                else:
                    with self.assertRaises(RuntimeError):
                        exec(program, namespace)
                self.assertEqual(calls, [lookup])
        old_guard.test, old_guard.body = new_guard.test, new_guard.body
        self.assertEqual(ast.dump(original), ast.dump(candidate))
        with self.assertRaisesRegex(AssertionError, 'boundary changed'):
            helpers()['sdk_baseline_with_current_space_policy'](modified)

    def test_only_directory_alias_failure_can_satisfy_behavioral_red(self):
        valid = ('FAILED (failures=1)\nRuntimeError: missing image file /usr/include/real: '
                 'Inode: 12 Type: directory\n')
        predicate = helpers()['sdk_alias_failure']
        self.assertTrue(predicate(valid))
        for output in ('FAILED (failures=1)\nSDK extraction needs 2 GiB estimated growth',
                       valid.replace('Type: directory', 'Type: regular'),
                       valid.replace('failures=1', 'errors=1'),
                       valid.replace('/usr/include/real:', '/unrelated:'),
                       *(valid + '\n' + error for error in ('SDK extraction needs 25 GiB',
                          'ModuleNotFoundError', 'ImportError', 'SyntaxError', 'No space left on device'))):
            with self.subTest(output=output):
                self.assertFalse(predicate(output))


if __name__ == '__main__':
    unittest.main()
