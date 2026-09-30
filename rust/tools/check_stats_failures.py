# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "pyzmq==27.2.0", "zstandard==0.25.0"]
# ///
# Run: PYTHONPATH=<original-msgq>:.:rust/tools python rust/tools/check_stats_failures.py BIN_DIR PARAMS_BINDING OUTPUT
"""Exercise failure boundaries using the real source/native daemon and filesystem."""
import json
import os
import signal
import subprocess
import sys
from pathlib import Path
import zmq
from logmessaged_native import Peer as Collector
from check_stats_daemon import ROOT, read_line


def scenario(binary, binding, output, original, case):
  output.mkdir(parents=True)
  collector = Collector(binary/'unused', output/'collector', original=True)
  collector.start()
  environment = dict(os.environ, OPENPILOT_PREFIX=collector.prefix, HOME=str(output/'home'), PARAMS_ROOT=str(output/'params'))
  params = output/'params'/collector.prefix
  params.mkdir(parents=True)
  if case == 'invalid-param':
    (params/'DongleId').write_bytes(b"invalid\xff'\n")
  metadata = output/'metadata'
  metadata.mkdir()
  (metadata/'build.json').write_text(json.dumps({'openpilot':{'version':'\ud800' if case=='surrogate' else 'v', 'git_origin':'git@github.com:test/openpilot.git'}}))
  stats = output/'stats'
  endpoint = 'ipc:///tmp/stats-fail-' + collector.prefix
  arguments = [endpoint,str(stats),str(metadata)]
  command = [sys.executable,str(ROOT/'rust/tools/stats_source.py'),*arguments,str(binding)] if original else [str(binary/'examples/stats_daemon'),*arguments]
  process = None
  try:
    with (output/'daemon.stderr').open('w') as stderr:
      process = subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr,text=True,env=environment)
    assert read_line(process,output)=='clock'
    with zmq.Context() as context, context.socket(zmq.PUSH) as producer:
      producer.setsockopt(zmq.LINGER,10)
      producer.setsockopt(zmq.SNDTIMEO,5000)
      producer.connect(endpoint)
      producer.send(b'bad\xff' if case=='utf8' else b'x:1|g')
      process.stdin.write('100\n'); process.stdin.flush()
      if case!='utf8':
        assert read_line(process,output)=='clock'
        process.stdin.write('161\n'); process.stdin.flush()
        assert read_line(process,output)=='clock'
        if case=='missing-directory':
          stats.rmdir()
        if case=='permission':
          stats.chmod(0o500)
        process.stdin.write('162\n'); process.stdin.flush()
        if case in ['collision','dangling']:
          assert read_line(process,output)=='clock'
          first=next(stats.iterdir())
          destination=first.with_name(first.name[:-1]+'1')
          if case=='collision':
            destination.write_text('existing-destination')
          else:
            destination.symlink_to(stats/'absent-target')
          producer.send(b'next:2|g')
          for value in [162,223]:
            process.stdin.write(str(value)+'\n');process.stdin.flush()
            assert read_line(process,output)=='clock'
          process.stdin.write('224\n');process.stdin.flush()
        if case in ['missing-param','invalid-param','dangling']:
          assert read_line(process,output)=='clock'
          process.stdin.close()
      code=process.wait(timeout=10)
      assert (code==0)==(case in ['missing-param','invalid-param','dangling']), (case,code)
    code,_=collector.stop(); assert code==-signal.SIGINT
    records=[json.loads(line) for path in collector.root.glob('swaglog.*') for line in path.read_text().splitlines()]
    files=[]
    if stats.exists():
      for path in sorted(stats.iterdir()):
        files.append({'content':path.read_bytes().hex(),'mode':path.stat().st_mode&0o777,'temporary':path.name.startswith('tmp')})
    result={'files':files,'logs':[{'msg':record.get('msg$s',record.get('msg')),'levelnum':record['levelnum']} for record in records]}
    if case=='utf8':
      assert result=={'files':[],'logs':[]}
    if case=='surrogate':
      assert result=={'files':[{'content':'','mode':0o600,'temporary':True}],'logs':[]}
    if case=='missing-directory':
      assert not stats.exists() and not result['logs']
    if case=='permission':
      assert result=={'files':[],'logs':[]}
      stats.chmod(0o700)
    if case=='collision':
      assert len(files)==2 and bytes.fromhex(files[1]['content'])==b'existing-destination'
    if case=='dangling':
      assert len(files)==2 and 'gauge.next,' in bytes.fromhex(files[1]['content']).decode()
      assert not any(path.is_symlink() for path in stats.iterdir())
    if case in ['missing-param','invalid-param']:
      assert len(files)==1 and 'dongle_id="None"' in bytes.fromhex(files[0]['content']).decode()
      assert len(result['logs'])==int(case=='invalid-param')
    (output/'result.json').write_text(json.dumps(result,indent=2))
    (output/'invocation.json').write_text(json.dumps({'argv':command,'environment':{key:environment[key] for key in ['OPENPILOT_PREFIX','HOME','PARAMS_ROOT']}},indent=2))
    return result
  finally:
    if process is not None and process.poll() is None:
      process.kill(); process.wait(timeout=5)
    collector.close()
    Path(endpoint[6:]).unlink(missing_ok=True)


def main():
  binary,binding,output=[Path(value).resolve() for value in sys.argv[1:]]
  results=[]
  for case in ['utf8','surrogate','missing-directory','permission','collision','dangling','missing-param','invalid-param']:
    source=scenario(binary,binding,output/(case+'-source'),True,case)
    native=scenario(binary,binding,output/(case+'-native'),False,case)
    assert source==native,(case,source,native)
    results.append({'case':case,'source':source,'native':native})
  (output/'comparison.json').write_text(json.dumps({'result':'PASS','scenarios':results},indent=2))
  print(f'{len(results)} source/native failure and Params scenarios PASS')


if __name__=='__main__':
  main()
