# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "pyzmq==27.2.0"]
# ///
# Run: PYTHONPATH=<original-msgq>:.:rust/tools python rust/tools/check_stats_flood.py BIN_DIR OUTPUT
"""Production daemon must observe shutdown while native metric producers remain busy."""
import hashlib
import json
import os
import selectors
import signal
import subprocess
import sys
import time
from pathlib import Path
from openpilot.cereal import messaging
from logmessaged_native import Peer as Collector


def cpu_ticks(pid):
  fields=Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()
  return int(fields[11])


def scenario(binary,output,signum):
  output.mkdir(parents=True)
  endpoint=Path('/tmp/stats')
  assert not endpoint.exists(), 'existing statistics endpoint must remain untouched'
  collector=Collector(binary/'unused',output/'collector',original=True)
  collector.start()
  environment=dict(os.environ,OPENPILOT_PREFIX=collector.prefix,HOME=str(output/'home'),PARAMS_ROOT=str(output/'params'))
  root=output/'source'
  root.mkdir()
  (root/'build.json').write_text(json.dumps({'openpilot':{'version':'v','git_origin':'git@github.com:test/openpilot.git'}}))
  stats=output/'home'/('.comma'+collector.prefix)/'stats'
  daemon=None
  producers=[]
  try:
    os.environ['OPENPILOT_PREFIX']=collector.prefix
    publisher=messaging.pub_sock('deviceState')
    with (output/'daemon.stdout').open('w') as stdout,(output/'daemon.stderr').open('w') as stderr:
      daemon=subprocess.Popen([str(binary/'statsd-rs')],cwd=root,env=environment,stdout=stdout,stderr=stderr)
    deadline=time.monotonic()+10
    while not stats.exists():
      assert daemon.poll() is None and time.monotonic()<deadline
      time.sleep(.01)
    baseline=cpu_ticks(daemon.pid)
    for index in range(2):
      with (output/f'producer-{index}.stderr').open('w') as stderr:
        producer=subprocess.Popen([str(binary/'examples/stats_flood'),'ipc:///tmp/stats'],stdout=subprocess.PIPE,stderr=stderr,text=True)
      producers.append(producer)
      with selectors.DefaultSelector() as selector:
        selector.register(producer.stdout,selectors.EVENT_READ)
        assert selector.select(10)
      assert producer.stdout.readline().strip()=='ready'
    while cpu_ticks(daemon.pid)-baseline < 10:
      assert daemon.poll() is None and all(producer.poll() is None for producer in producers)
      assert time.monotonic()<deadline
      time.sleep(.01)
    busy_ticks=cpu_ticks(daemon.pid)-baseline
    started=time.monotonic()
    daemon.send_signal(signum)
    try:
      code=daemon.wait(timeout=2)
    except subprocess.TimeoutExpired:
      code=None
    elapsed=time.monotonic()-started
    report={'signal':int(signum),'exit':code,'shutdown_seconds':elapsed,'busy_cpu_ticks':busy_ticks,
            'producers_alive':all(producer.poll() is None for producer in producers),
            'binary_sha256':hashlib.sha256((binary/'statsd-rs').read_bytes()).hexdigest(),
            'argv':daemon.args,'producer_argv':[producer.args for producer in producers]}
    (output/'result.json').write_text(json.dumps(report,indent=2))
    assert code==0 and report['producers_alive'],report
    assert not list(stats.iterdir()), 'shutdown must not add a flush'
    return report
  finally:
    for process in [daemon,*producers]:
      if process is not None and process.poll() is None:
        process.kill();process.wait(timeout=5)
    collector.stop();collector.close()
    endpoint.unlink(missing_ok=True)


def main():
  binary,output=[Path(value).resolve() for value in sys.argv[1:]]
  results=[scenario(binary,output/signum.name,signum) for signum in [signal.SIGINT,signal.SIGTERM]]
  (output/'comparison.json').write_text(json.dumps({'result':'PASS','scenarios':results},indent=2))
  print('SIGINT/SIGTERM stop a busy native drain while two native producers continue: PASS')


if __name__=='__main__':
  main()
