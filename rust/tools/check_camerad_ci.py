from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys

from card_qa.ci import ROOT, TOOLS, hashes, require_space, run

ASAN = Path('/usr/lib/llvm-18/lib/clang/18/lib/linux')
POLICIES = (
  ('sensor', 'sensors', 'sensor_policy'),
  ('exposure', 'exposure', 'exposure_policy'),
  ('misc', 'misc', 'misc_policy'),
  ('requests', 'requests', 'request_policy'),
  ('packets', 'packets', 'sensor_packets'),
)
LIFECYCLES = ('kernel', 'sensor_lifecycle', 'isp_lifecycle', 'camera_lifecycle')
RUNTIME_CASES = ('term', 'int', 'pwr', 'disabled-road', 'missing-road', 'wait-failure', 'poll-error', 'publication-order')
SCHEMAS = ('car', 'custom', 'deprecated', 'log')


class CameraCheck:
  def __init__(self, output: Path, binaries: Path, compiler: str) -> None:
    self.output = output
    self.binaries = binaries
    self.compiler = compiler
    self.commands = output / 'commands'

  def command(self, name: str, command: list[str], growth_mib: int = 64) -> Path:
    require_space(self.output, growth_mib * 1024**2)
    run(command, self.commands, name)
    return self.commands / f'{name}.stdout'

  def generate(self, name: str, script: str, arguments: tuple[str, ...] = ()) -> Path:
    stdout = self.command(f'generate-{name}', [sys.executable, str(TOOLS / script), '--source', str(ROOT), *arguments])
    target = self.output / f'{name}.cc'
    shutil.copyfile(stdout, target)
    return target

  def compile(self, name: str, sources: list[Path], *, sanitized: bool = False,
              shared: bool = False, libraries: tuple[str, ...] = ()) -> Path:
    flags = ['-std=c++17', '-O1' if sanitized else '-O2', '-pthread']
    for directory in (ROOT / 'openpilot', ROOT / 'third_party/linux/include', self.output / 'include',
                      self.output, self.output / 'cereal/gen/cpp', ROOT / 'msgq_repo'):
      flags.append('-I' + str(directory))
    if sanitized:
      flags += ['-g', '-fno-omit-frame-pointer', '-fsanitize=address,undefined', '-shared-libasan', '-Wl,-rpath,' + str(ASAN)]
    if shared:
      flags += ['-shared', '-fPIC']
    target = self.output / (name + ('.so' if shared else ''))
    self.command(f'compile-{name}', [self.compiler, *flags, *map(str, sources), *libraries, '-o', str(target)], 256)
    return target

  def stage(self) -> None:
    include = self.output / 'include/media'
    include.mkdir(parents=True)
    shutil.copyfile(ROOT / 'third_party/linux/include/msm_camsensor_sdk.h', include / 'msm_camsensor_sdk.h')
    schema = self.output / 'schema'
    (schema / 'include').mkdir(parents=True)
    for name in SCHEMAS:
      source = ROOT / ('opendbc_repo/opendbc/car' if name == 'car' else 'openpilot/cereal') / f'{name}.capnp'
      shutil.copyfile(source, schema / f'{name}.capnp')
    shutil.copyfile(ROOT / 'openpilot/cereal/include/c++.capnp', schema / 'include/c++.capnp')
    generated = self.output / 'cereal/gen/cpp'
    generated.mkdir(parents=True)
    self.command('compile-schema', ['capnp', 'compile', '-I', str(schema), '--src-prefix=' + str(schema),
      '-oc++:' + str(generated), *[str(schema / f'{name}.capnp') for name in SCHEMAS]], 128)

  def build(self) -> None:
    self.stage()
    sensors = [ROOT / f'openpilot/system/camerad/sensors/{name}.cc' for name in ('ar0231', 'ox03c10', 'os04c10')]
    cdm = ROOT / 'openpilot/system/camerad/cameras/cdm.cc'
    self.compile('sensor-source', [TOOLS / 'camerad_sensor_source.cc', *sensors])
    exposure = self.generate('exposure', 'generate_camerad_ae_reference.py')
    self.compile('exposure-source', [exposure, *sensors])
    misc = self.generate('misc', 'generate_camerad_ae_reference.py', ('--kind', 'misc'))
    self.compile('misc-source', [misc, cdm])
    requests = self.generate('requests', 'generate_camerad_requests_reference.py')
    self.compile('requests-source', [requests])
    packets = self.generate('packets', 'generate_camerad_packet_reference.py')
    self.compile('packets-source', [packets, cdm, *sensors])
    for name in LIFECYCLES:
      script = f'generate_camerad_{name}_reference.py'
      source = self.generate(name, script)
      sources = [source]
      if name != 'kernel':
        sources += sensors
      if name in ('isp_lifecycle', 'camera_lifecycle'):
        sources += [cdm]
      if name == 'camera_lifecycle':
        sources += [ROOT / 'msgq_repo/msgq' / part for part in (
          'msgq.cc', 'ipc.cc', 'event.cc', 'impl_msgq.cc', 'impl_fake.cc',
          'visionipc/visionipc.cc', 'visionipc/visionipc_client.cc', 'visionipc/visionipc_server.cc', 'visionipc/visionbuf.cc')]
      self.compile(f'{name}-source', sources, sanitized=True)
      fixture = TOOLS / 'camerad_kernel_fixture.cc' if name == 'kernel' else self.generate(f'{name}-fixture', script, ('--fixture',))
      self.compile(f'{name}-fixture', [fixture], sanitized=True, shared=True)
    state = self.generate('state', 'generate_camerad_state_reference.py')
    self.compile('state-source', [state, *sensors,
      *[self.output / f'cereal/gen/cpp/{name}.capnp.c++' for name in SCHEMAS]], sanitized=True, libraries=('-lcapnp', '-lkj'))
    stress = self.generate('stress', 'generate_camerad_stress_reference.py')
    self.compile('stress-source', [stress])
    self.compile('stress-fixture', [TOOLS / 'camerad_stress_fixture.cc'], shared=True, libraries=('-ldl',))
    runtime = self.generate('runtime-fixture', 'camerad_runtime_generate.py')
    self.compile('runtime-fixture', [runtime], sanitized=True, shared=True)

  def constants(self) -> None:
    for kind, tracked, arguments in (
      ('sensor', 'sensor/data.rs', ['--reference', str(self.output / 'sensor-source')]),
      ('bps', 'isp/bps_data.rs', []),
    ):
      stdout = self.command(f'constants-{kind}', [sys.executable, str(TOOLS / f'generate_camerad_{kind}_data.py'),
        '--source', str(ROOT), *arguments])
      generated = self.output / f'generated-{kind}.rs'
      shutil.copyfile(stdout, generated)
      actual = self.command(f'format-generated-{kind}', ['rustfmt', '--quiet', '--edition', '2021', '--emit', 'stdout', str(generated)])
      expected = self.command(f'format-tracked-{kind}', ['rustfmt', '--quiet', '--edition', '2021', '--emit', 'stdout',
        str(ROOT / 'rust/crates/camerad/src' / tracked)])
      if actual.read_bytes() != expected.read_bytes():
        raise AssertionError(f'{kind} generated constants differ; inspect retained formatted outputs')

  def check(self) -> None:
    examples = self.binaries / 'examples'
    for reference, checker, binary in POLICIES:
      self.command(f'check-{reference}', [sys.executable, str(TOOLS / f'check_camerad_{checker}.py'),
        '--reference', str(self.output / f'{reference}-source'), '--binary', str(examples / binary),
        '--output', str(self.output / f'result-{reference}')], 512)
    for name in LIFECYCLES:
      binary = 'kernel_operations' if name == 'kernel' else name
      command = [sys.executable, str(TOOLS / f'check_camerad_{name}.py'), '--source', str(self.output / f'{name}-source'),
        '--native', str(examples / binary), '--fixture', str(self.output / f'{name}-fixture.so')]
      self.command(f'check-{name}', [*command, '--output', str(self.output / f'result-{name}')], 512)
      if name == 'camera_lifecycle':
        self.command('check-camera-cleanup', [*command, '--cleanup-only', '--output', str(self.output / 'result-cleanup')], 512)
    self.command('check-state', [sys.executable, str(TOOLS / 'check_camerad_state.py'), '--source', str(self.output / 'state-source'),
      '--native', str(examples / 'frame_state'), '--schema', str(self.output / 'schema/log.capnp'),
      '--output', str(self.output / 'result-state')], 512)
    self.command('check-stress', [sys.executable, str(TOOLS / 'camerad_runtime_stress_check.py'),
      '--reference', str(self.output / 'stress-source'), '--native', str(examples / 'stress_trace'),
      '--parser', str(examples / 'stress_parse_trace'), '--fixture', str(self.output / 'stress-fixture.so'),
      '--output', str(self.output / 'result-stress')], 512)
    for case in RUNTIME_CASES:
      self.command(f'check-runtime-{case}', [sys.executable, str(TOOLS / 'camerad_runtime_check.py'),
        '--binary', str(self.binaries / 'openpilot-camerad'), '--fixture', str(self.output / 'runtime-fixture.so'),
        '--oracle', str(self.output / 'state-source'), '--case', case, '--output', str(self.output / f'runtime-{case}')], 512)


def main() -> None:
  parser = argparse.ArgumentParser(description='Rebuild camera source oracles and exercise the native runtime on a fresh host')
  parser.add_argument('--binaries', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--compiler', default='clang++-18')
  arguments = parser.parse_args()
  output = arguments.output.resolve()
  require_space(output, 2 * 1024**3)
  output.mkdir(parents=True)
  sources = list(TOOLS.glob('*camerad*'))
  for directory in ('rust/crates/camerad', 'rust/crates/camera-kernel', 'rust/crates/camerad-runtime', 'rust/crates/msgq',
                    'openpilot/system/camerad', 'msgq_repo/msgq'):
    sources += [path for path in (ROOT / directory).rglob('*') if path.is_file() and path.suffix in ('.rs', '.cc', '.h', '.toml')]
  binaries = arguments.binaries.resolve()
  executables = [binaries / 'openpilot-camerad'] + [binaries / 'examples' / name for name in (
    *[item[2] for item in POLICIES], 'kernel_operations', *LIFECYCLES[1:], 'frame_state', 'stress_trace', 'stress_parse_trace')]
  receipt = {'argv': sys.argv, 'source_sha256': hashes([path for path in sources if path.is_file()]),
    'binary_sha256': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in executables}}
  (output / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
  check = CameraCheck(output, binaries, arguments.compiler)
  check.build()
  check.constants()
  check.check()
  (output / 'result.json').write_text(json.dumps({'status': 'PASS', 'runtime_cases': RUNTIME_CASES,
    'scope': 'host source comparisons and native driver fixtures; no hardware or full startup acceptance'}) + '\n')


if __name__ == '__main__':
  main()
