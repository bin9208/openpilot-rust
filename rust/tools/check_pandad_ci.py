from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
import tarfile
import tomllib

from card_qa.ci import ROOT, TOOLS, hashes, require_space, run
from native_logging_build import stage_json11


def zmq_output(messages: Path) -> Path:
  outputs = set()
  for line in messages.read_text().splitlines():
    record = json.loads(line)
    if record.get('reason') == 'build-script-executed' and 'zmq-sys' in record['package_id']:
      outputs.add(Path(record['out_dir']).resolve())
  if len(outputs) != 1:
    raise ValueError(f'expected one executed zmq-sys build output, got {outputs}')
  output = outputs.pop()
  for name in ('source/include/zmq.h', 'lib/libzmq.a'):
    if not (output / name).is_file():
      raise FileNotFoundError(output / name)
  return output


class PandaCheck:
  def __init__(self, output: Path, binaries: Path) -> None:
    self.output = output
    self.binaries = binaries
    self.examples = binaries / 'examples'
    self.commands = output / 'commands'

  def command(self, name: str, arguments: list[str], growth_mib: int = 512) -> None:
    require_space(self.output, growth_mib * 1024**2)
    run(arguments, self.commands, name)

  def python(self, name: str, script: str, arguments: list[str]) -> None:
    self.command(name, [sys.executable, str(TOOLS / script), *arguments])

  def check(self, name: str, arguments: list[str], *, script: str | None = None) -> None:
    self.python(f'check-{name}', script or f'check_pandad_{name}.py',
      [*arguments, '--output', str(self.output / name)])

  def spidev(self) -> Path:
    package = next(p for p in tomllib.loads((ROOT / 'uv.lock').read_text())['package'] if p['name'] == 'spidev')
    locked = package['sdist']
    archive = self.output / locked['url'].rsplit('/', 1)[1]
    self.command('download-spidev', ['curl', '--retry', '3', '-fsSL', locked['url'], '-o', str(archive)], 64)
    actual = 'sha256:' + hashlib.sha256(archive.read_bytes()).hexdigest()
    if actual != locked['hash']:
      raise ValueError(f'spidev source hash mismatch: {actual} != {locked["hash"]}')
    extracted = self.output / 'spidev-source'
    require_space(self.output, 64 * 1024**2)
    with tarfile.open(archive) as source:
      source.extractall(extracted, filter='data')
    modules = list(extracted.glob('*/spidev_module.c'))
    if len(modules) != 1:
      raise ValueError(f'expected one original spidev module, got {modules}')
    (self.output / 'spidev-dependency.json').write_text(json.dumps({'version': package['version'],
      'url': locked['url'], 'hash': actual}, indent=2) + '\n')
    return modules[0].parent

  def execute(self, zmq: Path) -> None:
    require_space(self.output, 256 * 1024**2)
    json11, dependency = stage_json11(ROOT, self.output / 'json11-dependency')
    (self.output / 'json11-dependency.json').write_text(json.dumps(dependency, indent=2) + '\n')
    common = ['--json11-prefix', str(json11)]
    for mode in ('protocol', 'safety', 'device', 'state', 'can-io'):
      arguments = ['--output', str(self.output / f'{mode}-src'), '--capnp-prefix', '/usr', *common, '--sanitize']
      if mode != 'protocol':
        arguments.append('--' + mode)
      self.python(f'build-{mode}', 'build_pandad_protocol_source.py', arguments)
      name = mode.replace('-', '_')
      binary_flag = '--binary' if mode in ('protocol', 'safety') else '--native'
      binary = f'panda_{mode}' if mode in ('protocol', 'safety') else name
      check_arguments = ['--source', str(self.output / f'{mode}-src/pandad-protocol-source'),
        binary_flag, str(self.examples / binary)]
      self.check(name, check_arguments)
      if mode == 'safety':
        self.check('safety_padding', check_arguments)
    for name, example in (('peripheral', 'peripheral'), ('spi', 'spi_fixture')):
      self.python(f'build-{name}', f'build_pandad_{name}_source.py',
        ['--output', str(self.output / f'{name}-src'), *common, '--sanitize'])
      self.check(name, ['--source', str(self.output / f'{name}-src/pandad-{name}-source'), '--native', str(self.examples / example)])
    self.python('build-usb', 'build_pandad_usb_source.py',
      ['--output', str(self.output / 'usb-src'), *common, '--libusb-include', '/usr/include'])
    usb = self.output / 'usb-src/libpanda_usb_fixture.so'
    self.check('usb', ['--source', str(self.output / 'usb-src/pandad-usb-source'),
      '--binary', str(self.examples / 'usb_fixture'), '--fixture', str(usb)])
    for name, example in (('firmware', 'firmware'), ('client', 'firmware_client'), ('firmware_spi', 'firmware_spi'), ('supervisor', 'supervisor')):
      self.check(name, ['--binary', str(self.examples / example)])
    self.check('usb_policy', ['--binary', str(self.examples / 'usb_policy'), '--fixture', str(usb)])
    import usb1
    self.check('usb_raw', ['--binary', str(self.examples / 'raw_fixture'), '--fixture', str(usb), '--binding', str(Path(usb1.__file__).resolve())])
    self.python('build-spidev', 'build_pandad_firmware_spi_fixture.py',
      ['--output', str(self.output / 'spidev-src'), *common, '--spidev-source', str(self.spidev())])
    self.check('spidev', ['--native', str(self.examples / 'spi_policy'), '--library', str(self.output / 'spidev-src/libpandad_firmware_spi.so')])
    self.python('build-runtime-fixture', 'build_pandad_runtime_fixture.py',
      ['--output', str(self.output / 'runtime-fixture'), *common, '--libusb-include', '/usr/include'])
    runtime_usb = self.output / 'runtime-fixture/libusb-1.0.so.0'
    self.python('build-runtime-source', 'build_pandad_runtime_source.py', ['--output', str(self.output / 'runtime-src'),
      '--generated-root', str(self.output / 'protocol-src'), '--capnp-prefix', '/usr', *common,
      '--libusb-include', '/usr/include', '--zmq-root', str(zmq), '--fixture', str(runtime_usb), '--sanitize'])
    for name, binary, extra in (('source', self.output / 'runtime-src/pandad-source', ['--original']),
                                ('native', self.binaries / 'openpilot-pandad', [])):
      self.check(f'runtime-{name}', ['--binary', str(binary), '--collector', str(self.binaries / 'openpilot-logmessaged'),
        '--fixture', str(runtime_usb), *extra], script='check_pandad_runtime.py')
    self.python('compare-runtime', 'compare_pandad_runtime.py', ['--source', str(self.output / 'runtime-source'),
      '--native', str(self.output / 'runtime-native'), '--output', str(self.output / 'runtime-comparison.json')])
    child = self.output / 'supervisor-child'
    self.command('build-supervisor-child', ['clang++', '-std=c++17', '-O1', '-g', str(TOOLS / 'pandad_supervisor_child.cc'), '-o', str(child)], 64)
    for name in ('native_supervisor', 'supervisor_signals'):
      self.check(name, ['--binary', str(self.examples / 'supervisor_native'),
        '--launcher', str(self.binaries / 'openpilot-process-child'), '--library', str(usb), '--child', str(child)])
    self.check('dfu_recovery', ['--binary', str(self.examples / 'dfu_recovery'), '--fixture', str(usb)])
    for name, extra in (('explicit', []), ('default', ['--default-firmware'])):
      self.check(f'composed-{name}', ['--supervisor', str(self.binaries / 'openpilot-pandad-supervisor'),
        '--core', str(self.binaries / 'openpilot-pandad'), '--launcher', str(self.binaries / 'openpilot-process-child'),
        '--collector', str(self.binaries / 'openpilot-logmessaged'), '--fixture', str(runtime_usb), '--cli', *extra],
        script='check_pandad_composed.py')


def main() -> None:
  parser = argparse.ArgumentParser(description='Rebuild original Panda references and require native USB/SPI and supervisor comparisons')
  parser.add_argument('--binaries', type=Path, required=True)
  parser.add_argument('--build-messages', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  arguments = parser.parse_args()
  output, binaries = arguments.output.resolve(), arguments.binaries.resolve()
  require_space(output, 2 * 1024**3)
  output.mkdir(parents=True)
  zmq = zmq_output(arguments.build_messages)
  sources = list(TOOLS.glob('*pandad*'))
  for name in ('pandad', 'panda-usb', 'panda-spi', 'panda-spi-linux'):
    sources += list((ROOT / 'rust/crates' / name).rglob('*.rs'))
  executables = [binaries / name for name in ('openpilot-pandad', 'openpilot-pandad-supervisor', 'openpilot-process-child', 'openpilot-logmessaged')]
  executables += [binaries / 'examples' / name for name in ('panda_protocol', 'panda_safety', 'device', 'state', 'can_io',
    'peripheral', 'firmware', 'firmware_client', 'firmware_spi', 'supervisor', 'supervisor_native', 'dfu_recovery',
    'usb_policy', 'spi_policy', 'usb_fixture', 'raw_fixture', 'spi_fixture')]
  receipt = {'argv': sys.argv, 'source_sha256': hashes([path for path in sources if path.is_file()]), 'zmq_output': str(zmq),
    'binary_sha256': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in executables},
    'build_messages_sha256': hashlib.sha256(arguments.build_messages.read_bytes()).hexdigest()}
  (output / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
  PandaCheck(output, binaries).execute(zmq)
  (output / 'result.json').write_text(json.dumps({'status': 'PASS',
    'scope': 'host source comparisons and owned native fixtures; no physical USB/SPI, full startup or device acceptance'}) + '\n')


if __name__ == '__main__':
  main()
