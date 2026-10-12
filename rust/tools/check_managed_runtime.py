"""Exercise installed daemon failure boundaries through the actual native collector."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time


CASES = (
  ('openpilot-controlsd', 'controlsd', 'openpilot_controlsd::'),
  ('openpilot-radard', 'radard', 'openpilot_radard::'),
  ('openpilot-plannerd', 'plannerd', 'openpilot_plannerd::'),
  ('openpilot-paramsd', 'paramsd', 'openpilot_paramsd::'),
  ('openpilot-proclogd-runtime', 'proclogd', 'runtime::EntryError'),
  ('openpilot-uploader', 'uploader', 'openpilot_uploader::'),
  ('openpilot-bluetoothd', 'carrot_bluetooth', 'openpilot_bluetooth::'),
)


def digest(path: Path) -> str:
  with path.open('rb') as stream:
    return hashlib.file_digest(stream, 'sha256').hexdigest()


def run(binaries: Path, output: Path, binary: str, daemon: str, error_type: str, inherited_context: bool = True) -> dict:
  output.mkdir(parents=True)
  with tempfile.TemporaryDirectory(prefix='managed-runtime-') as directory:
    root = Path(directory)
    (root / 'build.json').write_text(json.dumps({'openpilot': {'git_origin': 'fixture/owned'}}))
    prefix = f'-managed-runtime-{os.getpid()}-{time.time_ns()}-{daemon}'
    endpoint_path = Path('/tmp') / f'logmessage{prefix}'
    namespace = Path('/dev/shm') / f'msgq_{prefix}'
    assert not endpoint_path.exists() and not namespace.exists()
    namespace.mkdir()
    log_root = output / 'logs'
    environment = {
      **os.environ,
      'OPENPILOT_PREFIX': prefix,
      'OPENPILOT_BASEDIR': str(root),
      'PARAMS_ROOT': str(root / 'params'),
      'RUST_BACKTRACE': '1',
      'MANAGER_DAEMON': daemon,
      'OPENPILOT_LOG_CONTEXT': json.dumps({'dongle_id': 'owned-device', 'version': 'owned-version'}),
    }
    if not inherited_context:
      environment.pop('OPENPILOT_LOG_CONTEXT')
    collector_environment = {key: value for key, value in environment.items() if key not in ('MANAGER_DAEMON', 'OPENPILOT_LOG_CONTEXT')}
    collector = None
    try:
      with (output / 'collector.log').open('wb') as stream:
        collector = subprocess.Popen(
          [str(binaries / 'openpilot-logmessaged'), '--log-root', str(log_root)], env=collector_environment, stdout=stream, stderr=stream
        )
      deadline = time.monotonic() + 10
      while not endpoint_path.exists() or not all((namespace / name).exists() for name in ('logMessage', 'errorLogMessage')):
        if collector.poll() is not None or time.monotonic() >= deadline:
          raise RuntimeError('owned collector did not initialize')
        time.sleep(0.01)
      result = subprocess.run([str(binaries / binary), '--invalid-managed-fixture'], env=environment, capture_output=True, timeout=15, check=False)
      (output / 'stdout.txt').write_bytes(result.stdout)
      (output / 'stderr.txt').write_bytes(result.stderr)
      assert result.returncode == 1, (binary, result.returncode, result.stderr)
      assert b'crash\n' in result.stderr, (binary, result.stderr)
      assert error_type.encode() in result.stderr, (binary, result.stderr)
      assert (root / 'params' / prefix / 'CarrotException').read_bytes() == b'exception'
      time.sleep(0.2)
      collector.send_signal(signal.SIGTERM)
      assert collector.wait(timeout=10) == 0
      records = [json.loads(line) for path in log_root.glob('swaglog.*') for line in path.read_text().splitlines()]
      crashes = [record for record in records if record.get('msg$s') == 'crash']
      assert len(crashes) == 1, records
      crash = crashes[0]
      context = crash['ctx']
      assert context['daemon'] == daemon, crash
      if inherited_context:
        assert context['dongle_id'] == 'owned-device', crash
        assert context['runtime_implementation'] == binary, crash
      assert context['runtime_language'] == 'rust', crash
      assert error_type in crash['exc_info'], crash
      assert 'disabled backtrace' not in crash['exc_info'], crash
      standalone = subprocess.run([str(binaries / binary), '--help'], env=collector_environment, capture_output=True, timeout=15, check=False)
      assert standalone.returncode == 0, (binary, standalone.stderr)
      return {
        'binary': binary,
        'sha256': digest(binaries / binary),
        'daemon': daemon,
        'typed_failure': error_type,
        'log_records': len(records),
        'exit_code': result.returncode,
        'carrot_exception': True,
        'standalone_help': True,
        'inherited_context': inherited_context,
      }
    finally:
      if collector is not None and collector.poll() is None:
        collector.kill()
        collector.wait()
      endpoint_path.unlink(missing_ok=True)
      if namespace.is_dir():
        for path in namespace.iterdir():
          path.unlink()
        namespace.rmdir()


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--binaries', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--daemon', choices=[row[1] for row in CASES], help='Run one focused daemon boundary')
  parser.add_argument('--omit-inherited-context', action='store_true', help='Exercise the managed entry without manager metadata')
  args = parser.parse_args()
  binaries, output = args.binaries.resolve(), args.output.resolve()
  rows = [
    run(binaries, output / daemon, binary, daemon, error_type, not args.omit_inherited_context)
    for binary, daemon, error_type in CASES
    if args.daemon is None or args.daemon == daemon
  ]
  receipt = {
    'result': 'PASS',
    'collector_sha256': digest(binaries / 'openpilot-logmessaged'),
    'scenarios': rows,
    'scope': 'owned host daemon entry, real log collector and Params; no hardware',
  }
  (output / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
