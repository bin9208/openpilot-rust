"""Owned native msgq peers drive the production hardwared entrypoint with a temporary PC root."""

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import threading
import time
import uuid

import msgq
import zmq
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='hardwared-owned-') as temporary:
    root = Path(temporary)
    for path, text in {
      'proc/stat': 'cpu 100 0 0 100 0 0 0 0 0 0\ncpu0 100 0 0 100 0 0 0 0 0 0\n',
      'proc/meminfo': 'MemTotal: 1000000 kB\nMemAvailable: 500000 kB\n',
      'data/params/d/HasAcceptedTerms': '2',
      'data/params/d/CompletedTrainingVersion': '0.2.0',
      'data/params/d/MaxTimeOffroadMin': '1800',
      'dev/kmsg': '',
    }.items():
      target = root / path
      target.parent.mkdir(parents=True, exist_ok=True)
      target.write_text(text)
    prefix = 'hardwared_' + uuid.uuid4().hex
    os.environ['OPENPILOT_PREFIX'] = prefix
    for key in ['ZMQ', 'CEREAL_FAKE']:
      os.environ.pop(key, None)
    shm = Path('/dev/shm/msgq_' + prefix)
    shm.mkdir()
    publishers = {
      name: msgq.pub_sock(name, SERVICE_LIST[name].queue_size) for name in ['pandaStates', 'peripheralState', 'selfdriveState', 'gpsLocationExternal']
    }
    receiver = msgq.sub_sock('deviceState', conflate=False, timeout=1000, segment_size=SERVICE_LIST['deviceState'].queue_size)
    context = zmq.Context()
    logs = context.socket(zmq.PULL)
    logs.setsockopt(zmq.LINGER, 0)
    logs.bind('ipc:///tmp/logmessage' + prefix)
    stats = context.socket(zmq.PULL)
    stats.setsockopt(zmq.LINGER, 0)
    stats.bind('ipc://' + str(root / 'stats'))
    process = None
    peer_stop = threading.Event()
    peer = None
    inputs = {"ignition": False, "engaged": False}
    packets, records, metrics = [], [], []
    try:
      with (args.output / 'daemon.log').open('w') as stderr:
        env = dict(os.environ, PATH=str(root / 'no-command-directory'))
        process = subprocess.Popen([str(args.binary.resolve()), '--root', str(root)], stdout=stderr, stderr=stderr, env=env)
        publishers['pandaStates'].wait_for_readers(timeout=8)

        def send(ignition, engaged=False):
          for service in ['peripheralState', 'selfdriveState', 'pandaStates']:
            event = log.Event.new_message(logMonoTime=time.monotonic_ns(), valid=False)
            if service == 'pandaStates':
              panda = event.init(service, 1)[0]
              panda.pandaType = 'dos'
              panda.harnessStatus = 'normal'
              panda.ignitionLine = ignition
            elif service == 'selfdriveState':
              event.init(service).enabled = engaged
            else:
              peripheral = event.init(service)
              peripheral.pandaType = 'dos'
              peripheral.voltage = 12000
            publishers[service].send(event.to_bytes())

        def publish_inputs():
          while not peer_stop.is_set():
            send(inputs['ignition'], inputs['engaged'])
            peer_stop.wait(0.1)

        peer = threading.Thread(target=publish_inputs)
        peer.start()

        def receive():
          raw = receiver.receive()
          assert raw is not None, ('missing deviceState', process.poll(), (args.output / 'daemon.log').read_text())
          (args.output / f'deviceState-{len(packets):03}.capnp').write_bytes(raw)
          with log.Event.from_bytes(raw) as event:
            assert event.which() == 'deviceState' and event.valid
            assert 0 <= time.monotonic() - event.logMonoTime / 1e9 < 2
            value = event.deviceState.to_dict()
            assert value['deviceType'] == 'pc'
            assert value['memoryUsagePercent'] == 50
            assert value['cpuUsagePercent'] == [0]
            packets.append(value)
            return value

        def until(started, ignition, engaged=False):
          deadline = time.monotonic() + 5
          inputs.update(ignition=ignition, engaged=engaged)
          while time.monotonic() < deadline:
            value = receive()
            if value['started'] == started:
              return value
          raise AssertionError(('state transition missing', started, packets))

        initial = until(False, False)
        onroad = until(True, True, True)
        params = root / 'data/params/d'
        deadline = time.monotonic() + 3
        while (params / 'IsEngaged').read_text() != '1':
          assert time.monotonic() < deadline
          receive()
        (params / 'OnroadCycleRequested').write_text('1')
        cycled = until(False, True)
        restarted = until(True, True)
        offroad = until(False, False)
        assert onroad['fanSpeedPercentDesired'] == 30 and offroad['fanSpeedPercentDesired'] == 0
        assert (params / 'OnroadCycleRequested').read_text() == '0'
        status_packet = json.loads((params / 'LastOffroadStatusPacket').read_text())
        assert status_packet['deviceState']['deviceState']['started'] is True
        assert 'deprecated' not in status_packet['deviceState']['deviceState']
        assert (params / 'UptimeOffroad').exists() and (params / 'UptimeOnroad').exists()
        process.send_signal(signal.SIGTERM)
        code = process.wait(timeout=5)
        assert code == 0, (code, (args.output / 'daemon.log').read_text())
        while logs.poll(100):
          raw = logs.recv()
          records.append(json.loads(raw[1:]))
        while stats.poll(100):
          metrics.append(stats.recv_string())
        assert any(row['msg'].get('event') == 'STATUS_PACKET' for row in records if isinstance(row['msg'], dict)), records
        assert any(metric.startswith('power_draw:') for metric in metrics), metrics
        assert any(metric.startswith('car_voltage:') for metric in metrics), metrics
        assert (params / 'NetworkMetered').read_text() == '0'
        (args.output / 'records.json').write_text(json.dumps(records, indent=2))
        (args.output / 'metrics.json').write_text(json.dumps(metrics, indent=2))
        (args.output / 'params.json').write_text(json.dumps({path.name: path.read_text() for path in params.iterdir() if path.is_file()}, indent=2))
        result = {
          'passed': True,
          'packets': len(packets),
          'transitions': [initial['started'], onroad['started'], cycled['started'], restarted['started'], offroad['started']],
          'exit_code': code,
          'validity_ignored_for_panda': True,
          'actual_board_access': False,
          'actual_host_commands': False,
        }
        (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        print('PASS native hardwared continuous IPC: offroad/onroad/cycle/restart/offroad, Params, stats, logging and SIGTERM')
    finally:
      peer_stop.set()
      if peer is not None:
        peer.join(timeout=3)
      if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=3)
      logs.close()
      stats.close()
      context.term()
      publishers.clear()
      receiver = None
      shutil.rmtree(shm)
      Path('/tmp/logmessage' + prefix).unlink(missing_ok=True)


if __name__ == '__main__':
  main()
