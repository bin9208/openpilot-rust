"""Startup order, original recovery/display handoff and unchanged boot cost."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('owned_user_startup', ROOT/'tools/native/user_startup.py')
user = importlib.util.module_from_spec(spec)
spec.loader.exec_module(user)


class Child:
  def __init__(self, code=None):
    self.code = code
    self.stdin = io.BytesIO()
    self.terminated = 0

  def poll(self): return self.code
  def terminate(self): self.terminated += 1; self.code = 0
  def kill(self): self.code = -9
  def wait(self, timeout=None): return self.code


class StartupTests(unittest.TestCase):
  def test_unusable_native_tools_fall_back_to_original_ui_and_recovery(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      ui = user.early.Startup(root, log=root/'launch_log')
      native_recovery, recovery = Child(127), Child()
      native_spinner, spinner = Child(127), Child()
      with mock.patch.object(ui, 'command', side_effect=[(['old-recovery'], True),
                                                       (['old-spinner'], True), (['old-text'], True)]), \
           mock.patch.object(user.early, 'listening', side_effect=[False, False, True]), \
           mock.patch.object(user.early.subprocess, 'Popen',
               side_effect=[native_recovery, recovery, native_spinner, spinner, Child(127), Child(0)]) as start:
        ui.start()
        self.assertIn('openpilot.selfdrive.carrot.recovery.server', start.call_args_list[1].args[0])
        self.assertIn('openpilot.system.ui.spinner', start.call_args_list[3].args[0])
        self.assertIn(b'Preparing build', spinner.stdin.getvalue())
        ui.failure('compile failed with old native UI')
        self.assertIn('openpilot.system.ui.text', start.call_args_list[5].args[0])
        ui.handoff()
        self.assertEqual(recovery.terminated, 0)

  def test_original_recovery_precedes_spinner_and_survives_the_manager_handoff(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      recovery, spinner = Child(), Child()
      ui = user.early.Startup(root, log=root/'launch_log')
      with mock.patch.object(user.early, 'listening', side_effect=[False, True]), \
           mock.patch.object(user.early.subprocess, 'Popen', side_effect=[recovery, spinner]) as start:
        ui.start()
        calls = start.call_args_list
        self.assertIn('openpilot.selfdrive.carrot.recovery.server', calls[0].args[0])
        self.assertEqual(calls[0].args[0][-2:], ['--port', '6999'])
        self.assertIn('openpilot.system.ui.spinner', calls[1].args[0])
        ui.update('Compiling changed source')
        self.assertIn(b'Compiling changed source', spinner.stdin.getvalue())
        ui.handoff()
        self.assertEqual(spinner.terminated, 1)
        self.assertEqual(recovery.terminated, 0)

  def test_failed_build_shows_original_error_screen_and_keeps_recovery_available(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      recovery, spinner, error = Child(), Child(), Child(0)
      ui = user.early.Startup(root, log=root/'launch_log')
      with mock.patch.object(user.early, 'listening', side_effect=[False, True]), \
           mock.patch.object(user.early.subprocess, 'Popen', side_effect=[recovery, spinner, error]) as start:
        ui.start()
        ui.failure('compiler rejected the changed source')
        argv = start.call_args_list[-1].args[0]
        self.assertIn('openpilot.system.ui.text', argv)
        self.assertIn('compiler rejected', argv[-1])
        self.assertIn('Recovery: port 6999', argv[-1])
        self.assertEqual(recovery.terminated, 0)
        ui.handoff()

  def test_machine_and_compile_preparation_cannot_precede_the_display_and_recovery(self):
    class Handoff(BaseException): pass
    order = []
    args = SimpleNamespace(source=ROOT, machine_config=Path('/owned/machine.json'),
        machine_configuration_sha256=None, running_config=None, previous=None, paths=None)
    display = mock.Mock()
    display.start.side_effect = lambda: order.append('display-and-recovery')
    display.handoff.side_effect = lambda: order.append('handoff')
    runtime = SimpleNamespace(PROGRESS=lambda *a, **k: None)
    functions = dict(runtime=runtime, seed=SimpleNamespace(native=lambda: order.append('native')),
        installed_machine_configuration=lambda *a: (order.append('machine') or ({}, 'pin')),
        validate_machine=lambda *a: order.append('configuration'))
    def execute(*args):
      order.append('exec')
      raise Handoff()
    with mock.patch.object(user, 'display_required', return_value=True), \
         mock.patch.object(user.early, 'Startup', return_value=display), \
         mock.patch.object(user.os, 'geteuid', return_value=1000), \
         mock.patch.object(user, 'compile_runtime', side_effect=lambda *a: (order.append('compile') or {})), \
         mock.patch.object(user.os, 'execve', side_effect=execute) as execve:
      with self.assertRaises(Handoff): user.launch(args, functions)
    self.assertEqual(order, ['display-and-recovery', 'native', 'machine', 'configuration', 'compile', 'handoff', 'exec'])
    self.assertEqual(execve.call_args.args[1][0], str(ROOT/'rust/bin/openpilot-launch'))
    self.assertEqual(execve.call_args.args[2]['OPENPILOT_USER_SOURCE_BUILD'], '1')
    self.assertNotIn('OPENPILOT_NATIVE_CONFIG', execve.call_args.args[2])

  def test_unchanged_start_neither_reopens_sdk_inputs_nor_runs_a_compiler(self):
    with tempfile.TemporaryDirectory() as temp:
      base = Path(temp)
      source, cache = base/'source', base/'cache'
      source.mkdir(); cache.mkdir()
      subprocess.run(['git', 'init', '-q', str(source)], check=True)
      (source/'README').write_text('owned source')
      subprocess.run(['git', '-C', str(source), 'add', '.'], check=True)
      subprocess.run(['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
          '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null', '-C', str(source),
          'commit', '-qm', 'source'], check=True)
      head = user.git(source, 'rev-parse', 'HEAD').decode().strip()
      tools = base/'tools'; tools.mkdir()
      helpers = {}
      for name in ('openpilot-runtime-native', 'openpilot-native-build-scope'):
        path = tools/name; path.write_text('not executed by an unchanged boot')
        helpers[str(path)] = user.stamp(path)
      binaries = source/'rust/bin'; binaries.mkdir(parents=True)
      binary = binaries/'openpilot-launch'; binary.write_text('previous compiler output')
      config = base/'configuration.json'; config.write_text('{}')
      state = dict(schema=1, source=str(source), inputs_key=user.inputs_key(source),
          configuration=str(config), configuration_sha256=hashlib.sha256(config.read_bytes()).hexdigest(),
          tools=str(tools), helpers=helpers, compiled_head=head, outputs={str(binary):user.stamp(binary)})
      user.save(cache/'user-startup.json', state)
      runtime = SimpleNamespace(read=lambda path, pin: json.loads(path.read_text()), PROGRESS=mock.Mock(),
          OwnedCommands=mock.Mock(side_effect=AssertionError('unchanged boot ran a build command')))
      functions = dict(runtime=runtime, seed=mock.Mock(),
          installed_checkout=mock.Mock(side_effect=AssertionError('unchanged boot reopened source/input preparation')),
          build_requested=mock.Mock(side_effect=AssertionError('unchanged boot rehashed SDK/provider inputs')))
      result = user.compile_runtime(SimpleNamespace(request_owner=None), functions, source, dict(cache=str(cache)), 'pin')
      self.assertEqual(result, state)
      functions['build_requested'].assert_not_called()
      runtime.OwnedCommands.assert_not_called()
      # The same device's completed dependency preparation survives helper/app
      # edits. Match real Git metadata without reading any SDK tree or archive.
      attempts = base/'generations/bootstrap-attempts/attempt-before/runtime-inputs'
      attempts.mkdir(parents=True)
      provider = base/'provider'; provider.mkdir()
      layout = base/'layout.json'; layout.write_text(json.dumps(dict(provider_root=str(provider))))
      plan = base/'plan.json'; plan.write_text('{}')
      compiler = base/'toolchain'; compiler.mkdir(); (compiler/'cargo').write_text('not executed')
      stores = dict(generations=str(base/'generations'))
      native = dict(stores=stores, cache=str(cache/'cargo-build'), toolchain=str(compiler),
          layout=str(layout), build_plan=str(plan), resources=dict(old='cap'))
      config.write_text(json.dumps(native))
      (attempts/'result.json').write_text(json.dumps(dict(source=dict(commit=head),
          configuration_sha256='selection-pin', native_configuration=str(config),
          native_configuration_sha256=hashlib.sha256(config.read_bytes()).hexdigest())))
      configuration = dict(stores=stores, cache=str(cache), runtime_inputs=dict(sha256='selection-pin'),
          resources=dict(current='cap'))
      runtime.read = lambda path, pin=None: json.loads(path.read_text())
      reused = user.reusable_preparation(configuration, source, runtime)
      self.assertEqual(reused, dict(native, resources=dict(current='cap')))
      before = user.inputs_key(source)
      def commit_file(name, text):
        path = source/name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text(text)
        subprocess.run(['git', '-C', str(source), 'add', name], check=True)
        subprocess.run(['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
            '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null', '-C', str(source),
            'commit', '-qm', 'changed source'], check=True)
      commit_file('tools/native/early_startup.py', 'changed display code')
      self.assertEqual(user.inputs_key(source), before, 'display changes must not restart helper/SDK preparation')
      commit_file('rust/crates/planner/src/main.rs', 'changed application code')
      self.assertEqual(user.inputs_key(source), before, 'Cargo handles application changes')
      commit_file('rust/crates/runtime-build/src/lib.rs', 'changed helper code')
      self.assertNotEqual(user.inputs_key(source), before, 'changed helpers must be rebuilt')
      before = user.inputs_key(source)
      commit_file('rust/crates/model-runtime/src/lib.rs', 'changed model compiler code')
      self.assertNotEqual(user.inputs_key(source), before, 'changed model compiler must refresh generated inputs')


if __name__ == '__main__':
  unittest.main()
