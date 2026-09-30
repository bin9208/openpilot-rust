# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "pyzmq==27.2.0", "zstandard==0.25.0"]
# ///
# Run: PYTHONPATH=<original-msgq>:.:rust/tools python rust/tools/check_stats_daemon.py BIN_DIR PARAMS_BINDING OUTPUT
"""Actual native metric PUSH, original deviceState peers, original/native continuous daemon."""
import hashlib
import json
import os
import selectors
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
import zmq
from openpilot.cereal import messaging, log
from logmessaged_native import Peer as Collector
ROOT = Path(__file__).resolve().parents[2]


def read_line(process, output):
  with selectors.DefaultSelector() as selector:
    selector.register(process.stdout, selectors.EVENT_READ)
    assert selector.select(10), (process.args, output)
  line = process.stdout.readline()
  assert line, (process.poll(), (output / 'daemon.stderr').read_text())
  with (output / 'protocol.jsonl').open('a') as stream:
    stream.write(json.dumps({'received': line.strip()}) + '\n')
  return line.strip()


def trial(binary_dir, binding, output, original):
  output.mkdir(parents=True)
  collector = Collector(binary_dir / 'unused', output / 'collector', original=True)
  collector.start()
  environment = dict(os.environ, OPENPILOT_PREFIX=collector.prefix, HOME=str(output/'home'), PARAMS_ROOT=str(output/'params'))
  (output/'params'/collector.prefix).mkdir(parents=True)
  (output/'params'/collector.prefix/'DongleId').write_bytes('fixture한'.encode())
  metadata = output/'metadata'
  metadata.mkdir()
  (metadata/'build.json').write_text(json.dumps({'channel': ['test', True, None], 'openpilot': {'version': {'v': 0.00001}, 'git_origin': 'https://github.com/test/openpilot.git'}}))
  stats = output/'stats'
  endpoint = 'ipc:///tmp/stats-' + collector.prefix
  arguments = [endpoint, str(stats), str(metadata)]
  command = [sys.executable, str(ROOT/'rust/tools/stats_source.py'), *arguments, str(binding)] if original else [str(binary_dir/'examples/stats_daemon'), *arguments]
  producer = process = None
  try:
    with (output/'daemon.stderr').open('w') as stderr:
      process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, env=environment)
    assert read_line(process, output)=='clock'
    os.environ['OPENPILOT_PREFIX'] = collector.prefix
    publisher = messaging.pub_sock('deviceState')
    with (output/'producer.stderr').open('w') as stderr:
      producer = subprocess.Popen([str(binary_dir/'examples/stats_producer'), endpoint], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True)
    def metrics(items):
      for item in items:
        producer.stdin.write(item+'\n'); producer.stdin.flush()
        assert producer.stdout.readline().strip()=='Sent'
    def tick(value):
      process.stdin.write(str(value)+'\n'); process.stdin.flush()
      assert read_line(process, output)=='clock'
    def device(started):
      packet = messaging.new_message('deviceState')
      packet.deviceState.started = started
      publisher.send(packet.to_bytes())
    def barrier(label, now):
      marker = 'barrier-' + label
      metrics([marker])
      deadline = time.monotonic() + 10
      while time.monotonic() < deadline:
        tick(now)
        while (packet := collector.subscribers['logMessage'].receive(non_blocking=True)) is not None:
          with log.Event.from_bytes(packet) as event:
            record = json.loads(event.logMessage)
            observed = event.logMonoTime
          if record['msg'] == {'event':'malformed metric','metric':marker}:
            (output/(marker+'.bin')).write_bytes(packet)
            with (output/'barriers.jsonl').open('a') as trace:
              trace.write(json.dumps({'marker':marker,'clock':now,'log_mono_time':observed})+'\n')
            return
      raise AssertionError(('metric-consumption barrier timed out', marker))
    metrics(['b:1|g','a:3|g','b:4|g','s:1e16|sa','s:1|sa','s:-1e16|sa','bad','x:bad|unknown','x:1|unknown','colon:1:ignored|g|ignored','unicode:１２_３.４|g'])
    metrics(['zero:-0|g','special:+Infinity|sa','special:-INFINITY|sa','special:nan|sa',
             'bad:1__2|g','bad:_1|g','bad:1_|g','space:\u00a01.5\u2003|g','bad:1\x1c|g'])
    import unicodedata
    metrics([f'digit{point}:{chr(point)}|g' for point in range(0x110000) if unicodedata.category(chr(point))=='Nd'])
    device(False)
    barrier("initial", 100)  # real collector ACK proves all preceding PUSH metrics were consumed
    tick(160)  # equality must not flush
    assert list(stats.iterdir())==[]
    tick(160.001)  # flush renders and asks for next last_flush time
    assert list(stats.iterdir())==[]
    tick(161)  # publication complete; next update
    first = list(stats.iterdir())
    assert len(first)==1 and first[0].name.endswith('_0')
    assert first[0].stat().st_mode & 0o777 == 0o600
    metrics(['after:9|g'])
    barrier('after', 161)
    device(True)
    for _ in range(6):
      tick(161)  # zero clock advance: only the deviceState transition can flush
      if len(list(stats.iterdir()))==2:
        break
    files = sorted(stats.iterdir())
    assert len(files)==2 and files[1].name.endswith('_1')
    assert 'started=True' in files[1].read_text()
    for index in range(9998):
      (stats/f'cap-{index}').touch()
    tick(223)  # empty flush still checks full directory
    tick(224)
    packet = collector.subscribers['errorLogMessage'].receive()
    assert packet is not None
    (output/'full-error.bin').write_bytes(packet)
    with log.Event.from_bytes(packet) as event:
      assert json.loads(event.errorLogMessage)['msg']=='stats dir full'
    metrics(['discarded:8|g'])
    barrier('discarded', 225)
    tick(286)
    tick(287)
    packet = collector.subscribers['errorLogMessage'].receive()
    assert packet is not None
    with log.Event.from_bytes(packet) as event:
      assert json.loads(event.errorLogMessage)['msg']=='stats dir full'
    for path in stats.glob('cap-*'):
      path.unlink()
    tick(348)
    tick(349)
    assert len(list(stats.iterdir()))==2  # full-directory flush discarded pending metrics
    process.stdin.close()
    assert process.wait(timeout=10)==0
    producer.stdin.close(); assert producer.wait(timeout=10)==0
    producer = None
    result = {'files': [path.read_text() for path in files], 'modes': [path.stat().st_mode&0o777 for path in files]}
    code, _ = collector.stop(); assert code == -signal.SIGINT
    records = [json.loads(line) for path in collector.root.glob('swaglog.*') for line in path.read_text().splitlines()]
    result['logs'] = [{'msg': record.get('msg$s', record.get('msg')), 'levelnum':record['levelnum']} for record in records]
    (output/'result.json').write_text(json.dumps(result, indent=2))
    (output/'invocation.json').write_text(json.dumps({'argv':command,'environment':{key:environment[key] for key in ['OPENPILOT_PREFIX','HOME','PARAMS_ROOT']},'source_sha256':hashlib.sha256((ROOT/'openpilot/system/statsd.py').read_bytes()).hexdigest()},indent=2))
    return result
  finally:
    for child in [producer,process]:
      if child is not None and child.poll() is None:
        child.kill(); child.wait(timeout=5)
    collector.close()
    Path(endpoint[6:]).unlink(missing_ok=True)


def main():
  binary, binding, output = [Path(value).resolve() for value in sys.argv[1:]]
  source = trial(binary,binding,output/'source',True)
  native = trial(binary,binding,output/'native',False)
  assert source==native, (source,native)
  (output/'comparison.json').write_text(json.dumps({'result':'PASS','source':source,'native':native},indent=2))
  print('continuous source/native comparison PASS')


if __name__=='__main__':
  main()
