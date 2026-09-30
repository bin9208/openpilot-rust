# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["numpy==2.5.3", "pycapnp==2.2.4", "pyzmq==27.2.0", "zstandard==0.25.0"]
# ///
# Run: PYTHONPATH=<original-msgq>:.:rust/tools python rust/tools/check_stats_log_fault.py BIN_DIR PARAMS_BINDING OUTPUT
"""One-shot log error must fall through the original metric handler to a real collector."""
import hashlib
import json
import os
import signal
import subprocess
import sys
from pathlib import Path
import zmq
from openpilot.cereal import log
from logmessaged_native import Peer as Collector
from check_stats_daemon import ROOT,read_line


def scenario(binary,binding,output,original):
  output.mkdir(parents=True)
  collector=Collector(binary/'unused',output/'collector',original=True)
  collector.start()
  environment=dict(os.environ,OPENPILOT_PREFIX=collector.prefix,HOME=str(output/'home'),PARAMS_ROOT=str(output/'params'))
  metadata=output/'metadata'
  metadata.mkdir()
  (metadata/'build.json').write_text(json.dumps({'openpilot':{'git_origin':'https://github.com/test/openpilot.git'}}))
  endpoint='ipc:///tmp/stats-logfault-'+collector.prefix
  process=None
  try:
    environment['STATS_ORACLE_LOG_FAILURE_TRACE']=str(output/'attempts.json')
    arguments=[endpoint,str(output/'stats'),str(metadata)]
    command=([sys.executable,str(ROOT/'rust/tools/stats_source.py'),*arguments,str(binding)] if original else [str(binary/'examples/stats_daemon'),*arguments])
    with (output/'daemon.stderr').open('w') as stderr:
      process=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr,text=True,env=environment)
    assert read_line(process,output)=='clock'
    with zmq.Context() as context,context.socket(zmq.PUSH) as producer:
      producer.setsockopt(zmq.LINGER,10)
      producer.setsockopt(zmq.SNDTIMEO,5000)
      producer.connect(endpoint)
      producer.send_string('x:1|unknown')
      process.stdin.write('100\n');process.stdin.flush()
      assert read_line(process,output)=='clock'
      attempts=json.loads((output/'attempts.json').read_text())
      process.stdin.close()
      assert process.wait(timeout=5)==0
    assert attempts==['unknown metric type','malformed metric']
    packet=collector.subscribers['logMessage'].receive()
    assert packet is not None
    (output/'logMessage.bin').write_bytes(packet)
    with log.Event.from_bytes(packet) as event:
      record=json.loads(event.logMessage)
    assert record['msg']=={'event':'malformed metric','metric':'x:1|unknown'}
    assert record['levelnum']==20
    code,_=collector.stop();assert code==-signal.SIGINT
    records=[json.loads(line) for path in collector.root.glob('swaglog.*') for line in path.read_text().splitlines()]
    assert len(records)==1
    result={'attempts':attempts,'msg':record['msg'],'levelnum':record['levelnum'],'disk_records':len(records)}
    (output/'result.json').write_text(json.dumps(result,indent=2))
    (output/'invocation.json').write_text(json.dumps({'argv':command,'fault':'first log attempt raises ZMQ EINVAL; successful calls retain original/native real logging'},indent=2))
    return result
  finally:
    if process is not None and process.poll() is None:
      process.kill();process.wait(timeout=5)
    collector.close()
    Path(endpoint[6:]).unlink(missing_ok=True)


def main():
  binary,binding,output=[Path(value).resolve() for value in sys.argv[1:]]
  source=scenario(binary,binding,output/'source',True)
  native=scenario(binary,binding,output/'native',False)
  assert source==native
  (output/'comparison.json').write_text(json.dumps({'result':'PASS','source':source,'native':native,
      'binary_sha256':hashlib.sha256((binary/'examples/stats_daemon').read_bytes()).hexdigest()},indent=2))
  print('unknown-log error falls back to actual malformed-metric collector record: PASS')


if __name__=='__main__':
  main()
