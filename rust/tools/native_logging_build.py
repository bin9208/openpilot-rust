#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run via check_native_logging.py; compiles the unchanged C++ producer and locked json11.
import hashlib
import json
import platform
import subprocess
import tomllib
import zipfile
from pathlib import Path


class UnsupportedArchitecture(ValueError):
  def __init__(self, architecture: str) -> None:
    self.architecture = architecture
    super().__init__(f'unsupported native json11 architecture: {architecture}')


def stage_json11(root: Path, output: Path) -> tuple[Path, dict]:
  output.mkdir(parents=True, exist_ok=True)
  lock = tomllib.loads((root / 'uv.lock').read_text())
  package = next(p for p in lock['package'] if p['name'] == 'comma-deps-json11')
  architecture = platform.machine()
  if architecture not in ('x86_64', 'aarch64'):
    raise UnsupportedArchitecture(architecture)
  wheel = next(w for w in package['wheels'] if f'manylinux_2_28_{architecture}' in w['url'])
  archive = output / wheel['url'].rsplit('/', 1)[1]
  subprocess.run(['curl', '-fsSL', wheel['url'], '-o', str(archive)], check=True)
  digest = hashlib.sha256(archive.read_bytes()).hexdigest()
  assert wheel['hash'] == f'sha256:{digest}'
  with zipfile.ZipFile(archive) as wheel_file:
    wheel_file.extractall(output / 'json11')
  install = next((output / 'json11').glob('*.data/purelib/json11/install'))
  return install, {'json11_version': package['version'], 'json11_sha256': digest, 'json11_architecture': architecture}


def build(root: Path, output: Path) -> Path:
  install, dependency = stage_json11(root, output)
  overlay = output / 'overlay'
  (overlay / 'common').mkdir(parents=True, exist_ok=True)
  (overlay / 'system/hardware').mkdir(parents=True, exist_ok=True)
  timing = (root / 'openpilot/common/timing.h').read_text()
  start = timing.index('static inline uint64_t nanos_since_boot()')
  end = timing.index('\n}', start) + 2
  timing = timing[:start] + 'extern uint64_t fixture_clock;\nstatic inline uint64_t nanos_since_boot() { return fixture_clock; }' + timing[end:]
  (overlay / 'common/timing.h').write_text(timing)
  (overlay / 'common/version.h').write_text('#define COMMA_VERSION "test-version"\n')
  (overlay / 'system/hardware/hw.h').write_text(
    '#include <cstdlib>\n#include <cstring>\n#include <string>\n' +
    'struct Hardware { static std::string get_name() { return "pc"; } };\n' +
    'struct Path { static std::string swaglog_ipc() { return getenv("NATIVE_LOG_ENDPOINT"); } };\n')
  zmq_include = next((root / 'rust/target/debug/build').glob('zmq-sys-*/out/source/include'))
  binary = output / 'native_logging_reference'
  command = ['c++', '-std=c++17', '-O2', '-pthread', f'-I{overlay}', f'-I{root / "openpilot"}',
             f'-I{install / "include"}', f'-I{zmq_include}', str(root / 'rust/tools/native_logging_reference.cc'),
             str(root / 'openpilot/common/swaglog.cc'), str(install / 'lib/libjson11.a'), '-l:libzmq.so.5',
             '-Wl,--wrap=zmq_send', '-Wl,--wrap=zmq_setsockopt', '-o', str(binary)]
  result = subprocess.run(command, check=True, capture_output=True, text=True)
  (output / 'build.log').write_text(result.stdout + result.stderr)
  provenance = {'command': command, **dependency,
                'sources': {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
                            for p in [root / 'openpilot/common/swaglog.cc', root / 'openpilot/common/swaglog.h',
                                      root / 'openpilot/common/timing.h', root / 'rust/tools/native_logging_reference.cc']},
                'clock_override': 'Only nanos_since_boot returns fixture_clock for exact rate-limit boundaries; realtime source unchanged.',
                'wrappers': 'zmq_send and zmq_setsockopt observe arguments/results and call actual libzmq unchanged.'}
  (output / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
  return binary
