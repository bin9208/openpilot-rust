"""Verify actual manager-started native logs through the existing uploader on loopback."""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
from urllib.parse import parse_qs, urlparse

import zstandard

from openpilot.cereal import log
from check_uploader import endpoint, signing_key


def run(root: Path, binaries: Path, probe: Path, output: Path) -> None:
  if shutil.disk_usage(root).free < 26 * 1024**3:
    raise RuntimeError('insufficient reserve for owned native executables')
  output.mkdir(parents=True, exist_ok=False)
  managed = output / 'manager'
  command = [str(probe), str(managed), str(root), str(binaries), 'log-route']
  with (output / 'manager-probe.log').open('wb') as stream:
    subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT, check=True, timeout=90)
  summary = json.loads((managed / 'summary.json').read_text())
  assert len(summary['native_children_reaped']) == 4
  qlogs = list((managed / 'logs').glob('*/qlog.zst'))
  assert len(qlogs) == 1, qlogs
  qlog = qlogs[0]
  rlog = qlog.with_name('rlog.zst')
  assert not list((managed / 'logs').rglob('*.lock'))
  content = zstandard.ZstdDecompressor().stream_reader(io.BytesIO(rlog.read_bytes())).read()
  messages = list(log.Event.read_multiple_bytes(content))
  records = [json.loads(str(event.logMessage)) for event in messages if event.which() == 'logMessage']
  native = [record for record in records if record.get('ctx', {}).get('daemon') == 'statsd']
  assert native, 'managed child log absent from native route'
  for record in native:
    assert record['ctx']['runtime_implementation'] == 'statsd-rs'
    assert record['ctx']['runtime_language'] == 'rust'
    assert record['ctx']['commit'] == summary['git_commit']
  qcontent = zstandard.ZstdDecompressor().stream_reader(io.BytesIO(qlog.read_bytes())).read()
  errors = [json.loads(str(event.errorLogMessage)) for event in log.Event.read_multiple_bytes(qcontent) if event.which() == 'errorLogMessage']
  crashes = [record for record in errors if record.get('msg') == 'crash' and record.get('ctx', {}).get('daemon') == 'statsd']
  assert len(crashes) == 1, crashes
  assert crashes[0]['ctx']['runtime_language'] == 'rust'
  assert crashes[0]['ctx']['runtime_implementation'] == 'statsd-rs'
  prefixes = list((managed / 'params').glob('rust-probe-installed-*'))
  assert len(prefixes) == 1
  prefix = prefixes[0].name
  (prefixes[0] / 'DongleId').write_text('0000000000000000')
  signing_key(managed / 'home' / f'.comma{prefix}' / 'persist', 'id_ecdsa')
  environment = {
    **os.environ,
    'HOME': str(managed / 'home'),
    'OPENPILOT_PREFIX': prefix,
    'PARAMS_ROOT': str(managed / 'params'),
    'LOG_ROOT': str(managed / 'logs'),
    'OPENPILOT_ROOT': str(root),
    'OPENPILOT_BASEDIR': str(root),
    'FORCEWIFI': '1',
    'UPLOADER_SLEEP': '0',
  }
  for key in ('FAKEUPLOAD', 'MANAGER_DAEMON', 'OPENPILOT_LOG_CONTEXT', 'ZMQ', 'CEREAL_FAKE'):
    environment.pop(key, None)
  captures = []
  namespace = Path('/dev/shm') / f'msgq_{prefix}'
  namespace.mkdir()
  try:
    with endpoint({}, captures) as host:
      result = subprocess.run(
        [str(binaries / 'openpilot-uploader'), '--cycles', '6'], env={**environment, 'API_HOST': host}, capture_output=True, timeout=20, check=False
      )
  finally:
    shutil.rmtree(namespace)
  (output / 'uploader.log').write_bytes(result.stdout + result.stderr)
  assert result.returncode == 0, result.stderr
  transfers = []
  for request, upload in zip(captures[::2], captures[1::2], strict=True):
    assert request[0] == 'GET' and upload[0] == 'PUT'
    path = parse_qs(urlparse(request[1]).query)['path'][0]
    transfers.append({'path': path, 'bytes': len(upload[3]), 'sha256': hashlib.sha256(upload[3]).hexdigest()})
    if path == str(qlog.relative_to(managed / 'logs')):
      assert upload[3] == qlog.read_bytes()
  assert any(row['path'] == str(qlog.relative_to(managed / 'logs')) for row in transfers), transfers
  assert os.getxattr(qlog, 'user.upload') == b'1'
  names = ('openpilot-manager', 'openpilot-process-child', 'openpilot-bootlog', 'openpilot-logmessaged', 'statsd-rs', 'openpilot-loggerd', 'openpilot-uploader')
  receipt = {
    'result': 'PASS',
    'manager_command': command,
    'manager_probe_sha256': hashlib.sha256(probe.read_bytes()).hexdigest(),
    'binaries': {name: hashlib.sha256((binaries / name).read_bytes()).hexdigest() for name in names},
    'source_commit': summary['git_commit'],
    'route_messages': len(messages),
    'managed_child_route_records': len(native),
    'managed_crashes_recorded_and_uploaded': len(crashes),
    'failed_child_restarted': True,
    'native_children_reaped': summary['native_children_reaped'],
    'transfers': transfers,
    'scope': 'actual manager, process launcher, bootlog, collector, statsd, route logger and uploader; owned IPC/storage/HTTP; other daemons blocked',
  }
  (output / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--binaries', type=Path, required=True)
  parser.add_argument('--probe', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  run(args.root.resolve(), args.binaries.resolve(), args.probe.resolve(), args.output.resolve())
