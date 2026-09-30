# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["numpy==2.5.3", "pycapnp==2.2.4", "pyzmq==27.2.0"]
# ///
# Run: PYTHONPATH=<original-msgq>:.:rust/tools python rust/tools/check_stats_runtime.py BIN_DIR OUTPUT
"""Run the production entrypoint with its real clocks, paths, and signal lifecycle."""
import hashlib
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path
import zmq
from openpilot.cereal import messaging
from logmessaged_native import Peer as Collector


def main():
  binary,output=[Path(value).resolve() for value in sys.argv[1:]]
  output.mkdir(parents=True)
  endpoint=Path('/tmp/stats')
  assert not endpoint.exists(), 'refusing to replace an existing statistics endpoint'
  collector=Collector(binary/'unused',output/'collector',original=True)
  collector.start()
  environment=dict(os.environ,OPENPILOT_PREFIX=collector.prefix,HOME=str(output/'home'),PARAMS_ROOT=str(output/'params'))
  root=output/'source'
  root.mkdir()
  (root/'build.json').write_text(json.dumps({'channel':'runtime-fixture','openpilot':{'version':'v','git_origin':'https://github.com/test/openpilot.git'}}))
  stats=output/'home'/('.comma'+collector.prefix)/'stats'
  process=None
  try:
    os.environ['OPENPILOT_PREFIX']=collector.prefix
    publisher=messaging.pub_sock('deviceState')
    with (output/'stdout.log').open('w') as stdout,(output/'stderr.log').open('w') as stderr:
      process=subprocess.Popen([str(binary/'statsd-rs')],cwd=root,env=environment,stdout=stdout,stderr=stderr)
    deadline=time.monotonic()+10
    while not stats.exists():
      assert process.poll() is None
      assert time.monotonic()<deadline
      time.sleep(.01)
    with zmq.Context() as context,context.socket(zmq.PUSH) as producer:
      producer.setsockopt(zmq.SNDTIMEO,5000)
      producer.setsockopt(zmq.LINGER,10)
      producer.connect('ipc:///tmp/stats')
      before=time.time_ns()
      producer.send_string('runtime:7|g')
      # Drive actual deviceState transitions until a file is observed; inputs are bounded.
      started=False
      while not list(stats.iterdir()):
        message=messaging.new_message('deviceState')
        message.deviceState.started=started
        publisher.send(message.to_bytes())
        started=not started
        assert process.poll() is None
        assert time.monotonic()<deadline
        time.sleep(.1)
      after=time.time_ns()
      files=list(stats.iterdir())
      assert len(files)==1
      text=files[0].read_text()
      assert text.startswith('gauge.runtime,started=') and ' value=7.0,dongle_id="None" ' in text
      stamp=int(text.rsplit(' ',1)[1])
      assert before<=stamp<=after and files[0].stat().st_mode&0o777==0o600
    process.send_signal(signal.SIGTERM)
    assert process.wait(timeout=5)==0
    assert len(list(stats.iterdir()))==1
    (output/'result.json').write_text(json.dumps({'result':'PASS','argv':process.args,'cwd':str(root),
        'binary_sha256':hashlib.sha256((binary/'statsd-rs').read_bytes()).hexdigest(),
        'before_ns':before,'file_timestamp_ns':stamp,'after_ns':after,'file':text,'shutdown_signal':'SIGTERM','exit':process.returncode},indent=2))
    print('production statsd-rs startup, real clock, file publication and SIGTERM PASS')
  finally:
    if process is not None and process.poll() is None:
      process.kill();process.wait(timeout=5)
    collector.stop();collector.close()
    endpoint.unlink(missing_ok=True)


if __name__=='__main__':
  main()
