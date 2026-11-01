from __future__ import annotations

import argparse
from collections.abc import Iterator
from contextlib import closing, contextmanager
import hashlib
import importlib
import importlib.metadata
import json
import os
from pathlib import Path
import select
import subprocess
import sys

from card_qa.ci import ROOT, TOOLS, hashes, require_space, run
from ci_cases import readonly_inputs, run_cases

GROUPS = {
  'policy-geometry': ('state', 'scheduler', 'qr', 'calibration', 'torque'),
  'pixels': ('root', 'augmented', 'camera', 'hud-eight-modes', 'hud-traffic'),
  'lifecycle-safety': ('runtime', 'pages', 'native-safety'),
}

EXAMPLES = ('product_render', 'product_runtime', 'state_trace', 'scheduler_trace', 'qr_trace',
  'calibration_geometry', 'torque_geometry')


def vision_peer(messages: Path) -> Path:
  outputs = set()
  for line in messages.read_text().splitlines():
    record = json.loads(line)
    if record.get('reason') == 'build-script-executed' and '#openpilot-msgq@' in record['package_id']:
      outputs.add(Path(record['out_dir']).resolve())
  if len(outputs) != 1:
    raise ValueError(f'expected one executed openpilot-msgq build output, got {outputs}')
  peer = outputs.pop() / 'native-vision-peer'
  if not peer.is_file():
    raise FileNotFoundError(peer)
  return peer


@contextmanager
def display_server(output: Path) -> Iterator[str]:
  read_fd, write_fd = os.pipe()
  with os.fdopen(read_fd) as reader, os.fdopen(write_fd, 'w') as writer, (output / 'xvfb.log').open('w') as log:
    command = ['Xvfb', '-displayfd', str(writer.fileno()), '-screen', '0', '2160x1080x24', '-nolisten', 'tcp']
    with subprocess.Popen(command, pass_fds=(writer.fileno(),), stdout=log, stderr=subprocess.STDOUT) as server:
      try:
        if not select.select([reader], [], [], 15)[0]:
          raise TimeoutError(f'Xvfb did not report a display; inspect {output / "xvfb.log"}')
        number = reader.readline().strip()
        if not number.isdecimal() or server.poll() is not None:
          raise RuntimeError(f'Xvfb display failed: {number!r}')
        display = ':' + number
        from Xlib.display import Display
        # Retain a group-owned client between transient application windows.
        # Closing the last client resets Xvfb, even while its process is alive.
        with closing(Display(display)):
          (output / 'display.json').write_text(json.dumps({'argv': command, 'pid': server.pid, 'display': display,
              'controller_pid': os.getpid(), 'controller_retained': True}) + '\n')
          yield display
      finally:
        if server.poll() is None:
          server.terminate()
        try:
          server.wait(timeout=10)
        except subprocess.TimeoutExpired:
          server.kill()
          server.wait()


class UiCheck:
  def __init__(self, output: Path, binaries: Path, peer: Path, group: str) -> None:
    self.group = group
    self.completed = []
    self.output = output
    self.binaries = binaries
    self.peer = peer
    self.commands = output / 'commands'

  def check(self, name: str, script: str, arguments: list[str]) -> None:
    if name not in GROUPS[self.group]:
      return
    require_space(self.output, 3 * 1024**3)
    run([sys.executable, str(TOOLS / script), *arguments], self.commands, name)
    self.completed.append(name)

  def execute(self, display: str) -> None:
    examples = self.binaries / 'examples'
    for name, binary in (('state', 'state_trace'), ('scheduler', 'scheduler_trace'), ('qr', 'qr_trace')):
      self.check(name, f'check_ui_{name}.py', ['--binary', str(examples / binary), '--output', str(self.output / name)])
    for name in ('root', 'augmented', 'camera'):
      self.check(name, f'check_ui_{name}.py', ['--binary', str(examples / 'product_render'),
        '--peer', str(self.peer), '--output', str(self.output / name), '--display', display])
    for name, script in (('runtime', 'check_ui_product_runtime.py'), ('pages', 'check_ui_runtime_pages.py')):
      self.check(name, script, [str(examples / 'product_runtime'), str(self.output / name), display, str(self.peer)])
    for name, arguments in (('hud-eight-modes', ['--large', '--filter', 'plot-modes']),
                            ('hud-traffic', ['--filter', 'traffic-numeric'])):
      self.check(name, 'check_ui_hud.py', ['--binary', str(examples / 'product_render'),
        '--output', str(self.output / name), '--display', display, *arguments])
    self.check('calibration', 'check_ui_calibration.py',
      [str(examples / 'calibration_geometry'), str(self.output / 'calibration'), display])
    self.check('torque', 'check_ui_torque_geometry.py',
      ['--binary', str(examples / 'torque_geometry'), '--output', str(self.output / 'torque')])
    self.check('native-safety', 'check_ui_native_safety.py', ['--target', str(self.binaries),
      '--output', str(self.output / 'native-safety'), '--display', display,
      '--raylib-root', os.environ['STARTUP_UI_RAYLIB_ROOT'], '--raylib-library', os.environ['STARTUP_UI_RAYLIB_LIBRARY']])


def main() -> None:
  parser = argparse.ArgumentParser(description='Run original/native UI comparisons and actual host application lifecycle on a fresh runner')
  parser.add_argument('--binaries', type=Path, required=True)
  parser.add_argument('--build-messages', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--group', choices=tuple(GROUPS))
  args = parser.parse_args()
  output = args.output.resolve()
  require_space(output, 4 * 1024**3)
  output.mkdir(parents=True)
  binaries = args.binaries.resolve()
  peer = vision_peer(args.build_messages)
  python = Path(os.environ['UI_MSGQ_PYTHON']).resolve()
  imports = {}
  for name in ('pyray', 'cffi', 'PIL', 'numpy', 'capnp', 'Xlib.ext.xtest', 'zmq', 'requests', 'qrcode',
               'setproctitle', 'msgq.ipc_pyx', 'msgq.visionipc.visionipc_pyx'):
    module = importlib.import_module(name)
    path = Path(module.__file__).resolve()
    imports[name] = str(path)
    if name.startswith('msgq.') and not path.is_relative_to(python):
      raise ValueError(f'{name} resolved outside the current Python binding directory: {path}')
  from openpilot.cereal import CEREAL_PATH, log
  log.Event.new_message()
  imports['cereal'] = str(CEREAL_PATH)
  source_paths = [Path(__file__), TOOLS/'ci_cases.py', ROOT / 'rust/Cargo.lock', ROOT / 'uv.lock']
  source_paths += list(TOOLS.glob('check_ui_*.py')) + list((TOOLS / 'ui_application_qa').glob('*.py'))
  for name in ('ui-application', 'ui-framework', 'startup-ui', 'msgq', 'params', 'hardware-info'):
    source_paths += [path for path in (ROOT / 'rust/crates' / name).rglob('*')
      if path.is_file() and path.suffix in ('.rs', '.cc', '.h', '.toml')]
  executables = [binaries / 'openpilot-ui', peer, Path(os.environ['STARTUP_UI_RAYLIB_LIBRARY'])]
  executables += [binaries / 'examples' / name for name in EXAMPLES]
  receipt = {'argv': sys.argv, 'source_sha256': hashes(source_paths), 'python_imports': imports,
    'versions': {name: importlib.metadata.version(name) for name in ('numpy', 'pillow', 'comma-deps-raylib',
      'cffi', 'python-xlib', 'pyzmq', 'pycapnp', 'requests', 'qrcode', 'setproctitle', 'Cython', 'setuptools')},
    'binary_sha256': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in executables},
    'environment': {key: os.environ.get(key) for key in ('PYTHONPATH', 'UI_MSGQ_PYTHON', 'STARTUP_UI_RAYLIB_ROOT',
      'STARTUP_UI_RAYLIB_LIBRARY', 'LIBGL_ALWAYS_SOFTWARE', 'CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS')}}
  (output / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
  if args.group is None:
    cases = [dict(name=name, argv=[sys.executable, str(Path(__file__).resolve()),
        '--binaries', str(binaries), '--build-messages', str(args.build_messages.resolve()),
        '--output', str(output/name), '--group', name],
        growth_bytes=3*1024**3, memory_bytes=(2 if name == 'lifecycle-safety' else 1)*1024**3)
        for name in GROUPS]
    with readonly_inputs([binaries, peer, Path(os.environ['STARTUP_UI_RAYLIB_ROOT']),
                          Path(os.environ['STARTUP_UI_RAYLIB_LIBRARY']), python]):
      run_cases(cases, output/'groups', ROOT)
    completed = []
    for name, expected in GROUPS.items():
      result = json.loads((output/name/'result.json').read_text())
      assert result['status'] == 'PASS' and set(result['cases']) == set(expected)
      completed.extend(result['cases'])
    assert len(completed) == len(set(completed)) == sum(map(len, GROUPS.values()))
    assert receipt['source_sha256'] == hashes(source_paths)
    assert receipt['binary_sha256'] == {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in executables}
    (output/'result.json').write_text(json.dumps(dict(status='PASS', groups=list(GROUPS), cases=completed,
        scope='all original host comparisons; target acceptance unchanged'))+'\n')
    return
  check = UiCheck(output, binaries, peer, args.group)
  run(['git', 'rev-parse', 'HEAD'], check.commands, 'source-commit')
  with display_server(output) as display:
    os.environ['DISPLAY'] = display
    run(['glxinfo', '-B'], check.commands, 'graphics')
    run(['xdpyinfo', '-ext', 'XTEST'], check.commands, 'xtest')
    from Xlib.display import Display
    with closing(Display(display)) as connection:
      if not connection.has_extension('XTEST'):
        raise RuntimeError(f'XTest unavailable on owned display {display}')
    check.execute(display)
  assert set(check.completed) == set(GROUPS[args.group]) and len(check.completed) == len(GROUPS[args.group])
  (output / 'result.json').write_text(json.dumps({'status': 'PASS', 'cases': check.completed,
    'scope': 'host original/native pixels, IPC, input, recording and cleanup; no target graphics or complete runtime acceptance'}) + '\n')


if __name__ == '__main__':
  main()
