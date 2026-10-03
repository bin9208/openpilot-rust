from __future__ import annotations

import argparse
import hashlib
import importlib
import importlib.metadata
import json
import os
from pathlib import Path
import subprocess
import sys

from card_qa.ci import ROOT, TOOLS, hashes, require_space, run
from native_logging_build import stage_json11


def dependency_outputs(messages: Path) -> dict[str, Path]:
  rows = [json.loads(line) for line in messages.read_text().splitlines()]
  finished = [row for row in rows if row.get('reason') == 'build-finished']
  if len(finished) != 1 or finished[0].get('success') is not True:
    raise ValueError('expected one successful completed Cargo build')
  result = {}
  for name, key, suffix, files in (
    ('openpilot-jpeg', 'jpeg', 'jpeg', ('libjpeg.a', 'jconfig.h')),
    ('zmq-sys', 'zmq', '', ('source/include/zmq.h', 'lib/libzmq.a')),
  ):
    outputs = {Path(row['out_dir']).resolve() for row in rows
               if row.get('reason') == 'build-script-executed' and f'#{name}@' in row['package_id']}
    if len(outputs) != 1:
      raise ValueError(f'expected one executed {name} build output, got {outputs}')
    output = outputs.pop() / suffix
    for relative in files:
      if not (output / relative).is_file():
        raise FileNotFoundError(output / relative)
    result[key] = output
  return result


def main() -> None:
  parser = argparse.ArgumentParser(description='Run the complete encoder host comparison against current Cargo artifacts')
  parser.add_argument('--binaries', type=Path, required=True)
  parser.add_argument('--build-messages', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--native', type=Path, default=Path('/'))
  parser.add_argument('--capnp-prefix', type=Path, default=Path('/usr'))
  parser.add_argument('--json11-prefix', type=Path, help='use an already staged native dependency')
  args = parser.parse_args()
  output, binaries, native = args.output.resolve(), args.binaries.resolve(), args.native.resolve()
  require_space(output, 1024**3)
  output.mkdir(parents=True, exist_ok=False)
  dependencies = dependency_outputs(args.build_messages)
  binding = Path(os.environ['ENCODER_MSGQ_PYTHON']).resolve()
  imports = {}
  for name in ('msgq.ipc_pyx', 'msgq.visionipc.visionipc_pyx', 'capnp', 'zmq'):
    module = importlib.import_module(name)
    path = Path(module.__file__).resolve()
    imports[name] = str(path)
    if name.startswith('msgq.') and not path.is_relative_to(binding):
      raise ValueError(f'{name} is outside the current original IPC bindings: {path}')
  from openpilot.cereal import CEREAL_PATH, log
  log.Event.new_message()
  imports['cereal'] = str(CEREAL_PATH)
  source_files = [ROOT / 'rust/Cargo.lock', ROOT / 'uv.lock', Path(__file__)]
  source_files += list(TOOLS.glob('*encoder*.py')) + list((TOOLS / 'encoder_oracle').glob('*'))
  for name in ('encoderd', 'msgq', 'jpeg'):
    source_files += [path for path in (ROOT / 'rust/crates' / name).rglob('*') if path.is_file()]
  selected = [binaries / 'openpilot-encoderd', *(binaries / 'examples' / name for name in ('encoder_codec_trace', 'encoder_v4l_trace'))]
  identity = {'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'sources': hashes(source_files), 'imports': imports,
              'python_packages': {name: importlib.metadata.version(name) for name in ('pycapnp', 'pyzmq')},
              'dependencies': {name: str(path) for name, path in dependencies.items()},
              'build_messages_sha256': hashlib.sha256(args.build_messages.read_bytes()).hexdigest(),
              'executables': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in selected},
              'native_codec_versions': subprocess.check_output(['pkg-config', '--modversion', 'libavcodec', 'libavformat', 'libavutil'], text=True)}
  (output / 'inputs.json').write_text(json.dumps(identity, indent=2) + '\n')
  json11 = args.json11_prefix
  if json11 is None:
    require_space(output, 64 * 1024**2)
    json11, dependency = stage_json11(ROOT, output / 'json11-dependency')
    (output / 'json11.json').write_text(json.dumps(dependency) + '\n')

  def check(name: str, script: str, arguments: list[str]) -> None:
    require_space(output, 1024**3)
    run([sys.executable, str(TOOLS / script), *arguments], output / 'commands', name)

  check('source-build', 'build_encoder_source.py', ['--output', str(output / 'source'), '--native-prefix', str(native / 'usr'),
    '--capnp-prefix', str(args.capnp_prefix.resolve()), '--json11-prefix', str(json11.resolve()),
    '--zmq-include', str(dependencies['zmq'] / 'source/include'), '--jpeg-build', str(dependencies['jpeg'])])
  check('codecs', 'check_encoder_codecs.py', ['--rust', str(selected[1]), '--output', str(output / 'codecs'),
    '--jpeg-build', str(dependencies['jpeg']), '--native', str(native)])
  check('v4l', 'check_encoder_v4l.py', ['--rust', str(selected[2]), '--output', str(output / 'v4l'), '--native', str(native)])
  check('runtime', 'check_encoder_runtime.py', ['--rust', str(selected[0]), '--source', str(output / 'source/original-encoderd'),
    '--output', str(output / 'runtime'), '--native', str(native)])
  for name, count in (('codecs', 5), ('v4l', 28), ('runtime', 10)):
    receipt = json.loads((output / name / 'receipt.json').read_text())
    if receipt['status'] != 'PASS' or len(receipt['results']) != count:
      raise AssertionError(f'incomplete encoder comparison: {name}')
  (output / 'receipt.json').write_text(json.dumps({'status': 'PASS', 'codec_cases': 5, 'v4l_cases': 28, 'runtime_cases': 10,
    'scope': 'source/native host codecs and IPC; target drivers, device performance and complete startup/upload remain separate'}) + '\n')


if __name__ == '__main__':
  main()
