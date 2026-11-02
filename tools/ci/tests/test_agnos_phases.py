import importlib.util
import ast
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('agnos_phases', Path(__file__).parents[1]/'agnos_phases.py')
phases = importlib.util.module_from_spec(spec)
spec.loader.exec_module(phases)


class PhaseTests(unittest.TestCase):
  def test_bound_provider_abi_preflight_keeps_all_elf_roots_and_rejects_incomplete_reports(self):
    tree = ast.parse((Path(__file__).parents[1]/'probe_native_agnos.py').read_text())
    function = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'native_provider_layout_abi')
    namespace = dict(Path=Path, json=json, digest=lambda p: hashlib.sha256(p.read_bytes()).hexdigest())
    exec(compile(ast.Module(body=[function], type_ignores=[]), '<provider-preflight>', 'exec'), namespace)
    with tempfile.TemporaryDirectory() as temporary:
      work = Path(temporary)
      for name in ('lib/central.so', 'torqued-numerics/blas.so', 'torqued-numerics/fortran.so', 'licenses/provider.txt'):
        path = work/'prefix'/name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b'\x7fELFfixture' if name.endswith('.so') else b'original license')
      layout = dict(provider_root='/guest/prefix', files={name: name for name in (
          'lib/central.so', 'torqued-numerics/blas.so', 'torqued-numerics/fortran.so', 'licenses/provider.txt')},
          libraries=['/factory/second', '/factory/first'])
      observed = []
      def execute(argv, name):
        observed.append((argv, name))
        roots = [str(argv[i+1]) for i, value in enumerate(argv) if value == '--root']
        output = argv[argv.index('--output')+1]
        output.write_text(json.dumps(dict(compatible=True,
            roots=[dict(root=root, compatible=True, failures=[]) for root in roots])))
      result = namespace['native_provider_layout_abi'](work, '/guest', layout, execute)
      self.assertEqual((result['success'], result['roots'], result['full_runtime_build']), (True, 3, False))
      self.assertEqual(len(observed), 1)
      argv, _ = observed[0]
      self.assertEqual([str(argv[i+1]) for i, value in enumerate(argv) if value == '--library'],
          ['/guest/prefix/lib', '/guest/prefix/torqued-numerics', '/factory/second', '/factory/first'])
      for bad in (dict(compatible=False, roots=[]), dict(compatible=True, roots=[])):
        def incompatible(argv, name):
          argv[argv.index('--output')+1].write_text(json.dumps(bad))
        with self.assertRaises(AssertionError):
          namespace['native_provider_layout_abi'](work, '/guest', layout, incompatible)
      def failed(argv, name):
        raise RuntimeError('missing provider')
      with self.assertRaisesRegex(RuntimeError, 'missing provider'):
        namespace['native_provider_layout_abi'](work, '/guest', layout, failed)

  def test_probe_does_not_shadow_called_imports_with_local_loop_variables(self):
    tree = ast.parse((Path(__file__).parents[1]/'probe_native_agnos.py').read_text())
    imports = {a.asname or a.name for n in tree.body if isinstance(n, (ast.Import, ast.ImportFrom)) for a in n.names}
    main = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'main')
    assigned = {n.id for n in ast.walk(main) if isinstance(n, ast.Name) and isinstance(n.ctx, ast.Store)}
    called = {n.func.id for n in ast.walk(main) if isinstance(n, ast.Call) and isinstance(n.func, ast.Name)}
    self.assertFalse(imports & assigned & called)

  def test_parallel_child_and_components_both_complete_before_final(self):
    with tempfile.TemporaryDirectory() as temporary:
      work, evidence = Path(temporary), dict(commands=[], source_commit='a'*40)
      def bootstrap(current, log, *, runner):
        current['commands'].append(runner([sys.executable, '-c', 'import time; time.sleep(0.2)'],
            log, stdout_path=work/'bootstrap.stdout'))
        current['native_bootstrap_current_tools'] = dict(actual_current_tools=13)
      with mock.patch.object(phases, 'parallel_admission', return_value=dict(parallel=True)):
        with phases.independent_bootstrap(bootstrap, work, work, evidence, work/'log'):
          self.assertNotIn('native_bootstrap_current_tools', evidence)
          time.sleep(0.05)
        self.assertEqual(evidence['native_bootstrap_current_tools']['actual_current_tools'], 13)
        self.assertEqual(evidence['phase_times'][0]['state'], 'success')

  def test_insufficient_combined_admission_uses_sequential_proof(self):
    with tempfile.TemporaryDirectory() as temporary:
      work, evidence = Path(temporary), dict(commands=[], source_commit='a'*40)
      def bootstrap(current, log): current['native_bootstrap_current_tools'] = dict(success=True)
      with mock.patch.object(phases, 'parallel_admission', return_value=dict(parallel=False)):
        with phases.independent_bootstrap(bootstrap, work, work, evidence, work/'log'):
          self.assertTrue(evidence['native_bootstrap_current_tools']['success'])
      self.assertEqual(evidence['phase_times'][0]['name'], 'bootstrap-current-tools-sequential')

  def test_failed_bootstrap_never_reaches_final(self):
    with tempfile.TemporaryDirectory() as temporary:
      work, evidence = Path(temporary), dict(commands=[], source_commit='a'*40)
      def bootstrap(current, log, *, runner):
        runner([sys.executable, '-c', 'raise SystemExit(7)'], log, stdout_path=work/'stdout')
      with mock.patch.object(phases, 'parallel_admission', return_value=dict(parallel=True)):
        with self.assertRaisesRegex(RuntimeError, 'bootstrap proof failed'):
          with phases.independent_bootstrap(bootstrap, work, work, evidence, work/'log'): pass
      self.assertEqual(evidence['phase_times'][0]['state'], 'failed')
      self.assertNotIn('native_bootstrap_current_tools', evidence)


if __name__ == '__main__': unittest.main()
