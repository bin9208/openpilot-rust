#!/usr/bin/env python3
"""Exercise the native uploader with original msgq, synthetic Params and local HTTP."""

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

import msgq
import zmq
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from check_uploader import endpoint, make_files, signing_key


def marked(path):
  try:
    return os.getxattr(path, 'user.upload') == b'1'
  except OSError:
    return False


def scenario(binary, output, name, config):
  destination = output / name
  destination.mkdir(parents=True)
  shm = Path(tempfile.mkdtemp(prefix='msgq_uploader-', dir='/dev/shm'))
  prefix = shm.name.removeprefix('msgq_')
  os.environ['OPENPILOT_PREFIX'] = prefix
  os.environ.pop('ZMQ', None)
  os.environ.pop('CEREAL_FAKE', None)
  publisher = msgq.pub_sock('deviceState', SERVICE_LIST['deviceState'].queue_size)
  context = zmq.Context()
  collector = context.socket(zmq.PULL)
  collector.setsockopt(zmq.LINGER, 0)
  collector.bind('ipc:///tmp/logmessage' + prefix)
  messages = []
  process = None
  try:
    with tempfile.TemporaryDirectory(prefix='uploader-daemon-') as temporary:
      temporary = Path(temporary)
      home = temporary / 'home'
      params = temporary / 'params' / prefix
      params.mkdir(parents=True)
      (params / 'DongleId').write_text('0000000000000000')
      (params / 'IsOffroad').write_text('1')
      root = temporary / 'logs'
      root.mkdir()
      make_files(root, [('route--0/qlog.lock', 0, False)])
      signing_key(home / ('.comma' + prefix) / 'persist', 'id_ecdsa')
      captures = []
      with endpoint(config, captures) as host, (destination / 'daemon.log').open('w') as output_log:
        environment = dict(
          os.environ, HOME=str(home), LOG_ROOT=str(root), PARAMS_ROOT=str(params.parent), API_HOST=host, UPLOADER_SLEEP='1' if config.get('sleep') else '0'
        )
        for key in ['FORCEWIFI', 'FAKEUPLOAD']:
          environment.pop(key, None)
        if config.get('force'):
          environment['FORCEWIFI'] = ''
        if config.get('fake'):
          environment['FAKEUPLOAD'] = ''
        process = subprocess.Popen([binary], env=environment, stdout=output_log, stderr=output_log)
        publisher.wait_for_readers(timeout=5)
        assert process.poll() is None, (name, (destination / 'daemon.log').read_text())
        assert not (root / 'route--0/qlog.lock').exists()
        packet = log.Event.new_message()
        packet.logMonoTime = time.monotonic_ns()
        packet.valid = False  # Source uploader consumes latest network fields regardless of validity.
        device = packet.init('deviceState')
        device.networkType = config.get('network', 'wifi')
        device.networkMetered = config.get('metered', False)
        raw = packet.to_bytes()
        (destination / 'device-state.capnp').write_bytes(raw)
        if config.get('sleep'):
          # The source's no-network idle wait does not wake on an incoming message.
          publisher.send(raw)
          time.sleep(0.15)
          assert not captures and not marked(root / 'route--0/qlog')
        else:
          publisher.send(raw)
          publisher.wait_for_readers(timeout=3)
          lock = root / 'route--0/new.lock'
          lock.write_bytes(b'')
          make_files(root, [('route--0/qlog', 2048, False), ('route--0/qcamera.ts', 1031, False)])
          lock.unlink()
          deadline = time.monotonic() + 5
          while not marked(root / 'route--0/qlog'):
            publisher.send(raw)
            assert process.poll() is None and time.monotonic() < deadline, (name, captures, (destination / 'daemon.log').read_text())
            time.sleep(0.01)
          if config.get('metered'):
            time.sleep(0.05)
            assert not marked(root / 'route--0/qcamera.ts')
            (params / 'AthenadRecentlyViewedRoutes').write_text('synthetic-dongle|route')
          deadline = time.monotonic() + 5
          while not marked(root / 'route--0/qcamera.ts'):
            publisher.send(raw)
            assert process.poll() is None and time.monotonic() < deadline
            time.sleep(0.01)
        while collector.poll(50):
          message = collector.recv()
          (destination / f'log-{len(messages):03d}.packet').write_bytes(message)
          messages.append(json.loads(message[1:]))
        start = time.monotonic()
        process.send_signal(signal.SIGINT if config.get('sigint') else signal.SIGTERM)
        assert process.wait(timeout=2) == 0
        signal_latency = time.monotonic() - start
        assert signal_latency < 1
        uploads = [item for item in captures if item[0] == 'PUT']
        if not config.get('sleep'):
          assert len(uploads) == (0 if config.get('fake') else 2), [(item[0], item[1]) for item in captures]
          successful = [row for row in messages if isinstance(row['msg'], dict) and row['msg'].get('event') == 'upload_success']
          assert len(successful) == 2, messages
          for row in successful:
            assert row['process'] == process.pid
            assert row['ctx']['runtime_language'] == 'rust'
            assert row['msg']['network_type'] == (0 if config.get('force') and config.get('network') == 'none' else 1)
            assert row['msg']['metered'] == config.get('metered', False)
        for index, upload in enumerate(uploads):
          (destination / f'upload-{index}.bin').write_bytes(upload[3])
        record = {
          'passed': True,
          'name': name,
          'exit_code': process.returncode,
          'signal_seconds': signal_latency,
          'requests': [{'method': method, 'path': path, 'bytes': len(body)} for method, path, _, body in captures],
          'log_packets': len(messages),
          'queue_bytes': (shm / 'deviceState').stat().st_size,
          'cleared_startup_lock': True,
        }
        (destination / 'report.json').write_text(json.dumps(record, indent=2) + '\n')
        return record
  finally:
    if process is not None and process.poll() is None:
      process.kill()
      process.wait()
    del publisher
    collector.close()
    context.term()
    Path('/tmp/logmessage' + prefix).unlink(missing_ok=True)
    shutil.rmtree(shm)


def startup_failures(binary, output):
  results = []
  with tempfile.TemporaryDirectory(prefix='uploader-startup-') as temporary:
    root = Path(temporary)
    logs = root / 'logs'
    logs.mkdir()
    params = root / 'params' / 'd'
    params.mkdir(parents=True)
    for name, extra in [('missing-id', {}), ('invalid-sleep', {'UPLOADER_SLEEP': 'invalid'}), ('missing-root', {'LOG_ROOT': str(root / 'absent')})]:
      environment = dict(os.environ, HOME=str(root), PARAMS_ROOT=str(params.parent), LOG_ROOT=str(logs), UPLOADER_SLEEP='0')
      environment.pop('OPENPILOT_PREFIX', None)
      environment.update(extra)
      run = subprocess.run([binary, '--cycles', '1'], env=environment, capture_output=True, timeout=3)
      (output / (name + '.log')).write_bytes(run.stdout + run.stderr)
      assert run.returncode != 0
      results.append({'name': name, 'exit_code': run.returncode})
  return results


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.binary = args.binary.resolve()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  rows = []
  for name, config in [
    ('wifi', {}),
    ('metered-requested', {'metered': True}),
    ('force-none-metered', {'force': True, 'network': 'none', 'metered': True}),
    ('fake', {'fake': True}),
    ('no-network-sigterm', {'sleep': True}),
    ('no-network-sigint', {'sleep': True, 'sigint': True}),
  ]:
    rows.append(scenario(args.binary, args.output, name, config))
  report = {'passed': True, 'scenarios': rows, 'startup_errors': startup_failures(args.binary, args.output)}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report, indent=2))


if __name__ == '__main__':
  main()
