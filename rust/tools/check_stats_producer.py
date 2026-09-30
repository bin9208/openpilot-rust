# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pycapnp==2.1.0", "pyzmq==27.2.0"]
# ///
# Run: python rust/tools/check_stats_producer.py EXAMPLES_DIR OUTPUT
"""Real ZMQ peer verifies original/native fork reconnect and inherited-handle destruction."""
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path
import zmq


def main():
  binary, output = [Path(value).resolve() for value in sys.argv[1:]]
  output.mkdir(parents=True)
  results = []
  with zmq.Context() as context:
    for original in [True, False]:
      for mode in ['reconnect', 'drop']:
        endpoint = f'ipc:///tmp/stats-fork-{os.getpid()}'
        with context.socket(zmq.PULL) as receiver:
          receiver.setsockopt(zmq.RCVTIMEO, 5000)
          receiver.bind(endpoint)
          command = ([sys.executable,str(Path(__file__).with_name('stats_source_producer.py'))] if original else [str(binary/'stats_fork')]) + [endpoint,mode]
          with (output/f'{original}-{mode}.stderr').open('w') as stderr:
            child = subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr,text=True)
          try:
            messages=[]
            for expected in (['parent-before','parent-after'] if mode=='drop' else ['parent-before','child-record','parent-after']):
              message=receiver.recv_string()
              assert message==expected
              messages.append(message)
              child.stdin.write('ack\n');child.stdin.flush()
            assert child.wait(timeout=10)==0
            results.append({'original':original,'mode':mode,'messages':messages,'argv':command})
          finally:
            if child.poll() is None:
              child.kill(); child.wait(timeout=5)
          receiver.unbind(endpoint)
        Path(endpoint[6:]).unlink(missing_ok=True)
    for original in [True, False]:
      endpoint = f'ipc:///tmp/stats-pressure-{os.getpid()}'
      command = ([sys.executable,str(Path(__file__).with_name('stats_source_producer.py')),endpoint,'pressure'] if original else [str(binary/'stats_pressure'),endpoint])
      with (output/f'pressure-{original}.stderr').open('w') as stderr:
        child=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr,text=True)
      try:
        assert child.stdout.readline().strip()=='ready'
        with context.socket(zmq.PULL) as receiver:
          receiver.setsockopt(zmq.RCVTIMEO,5000)
          receiver.bind(endpoint)
          messages=[receiver.recv_string() for _ in range(1000)]
          assert messages==[f'pressure:{float(index)}|g' for index in range(1000)]
          assert receiver.poll(100)==0
          child.stdin.write('ack\n');child.stdin.flush()
          assert child.wait(timeout=5)==0
        results.append({'original':original,'mode':'pressure','accepted':len(messages),'dropped':19000,'argv':command})
      finally:
        if child.poll() is None:
          child.kill();child.wait(timeout=5)
        Path(endpoint[6:]).unlink(missing_ok=True)
    typed = []
    for original in [True, False]:
      endpoint = f'ipc:///tmp/stats-types-{os.getpid()}'
      with context.socket(zmq.PULL) as receiver:
        receiver.setsockopt(zmq.RCVTIMEO,5000)
        receiver.bind(endpoint)
        command = ([sys.executable,str(Path(__file__).with_name('stats_source_producer.py')),endpoint,'types'] if original else [str(binary/'stats_types'),endpoint])
        with (output/f'types-{original}.stderr').open('w') as stderr:
          child=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr,text=True)
        try:
          messages=[]
          for _ in range(10):
            messages.append(receiver.recv_string())
            child.stdin.write('ack\n');child.stdin.flush()
          assert child.wait(timeout=5)==0
          assert messages[0]=='gpu_usage_percent:6|g'
          assert messages[-2]=='integer_sample:6|sa'
          typed.append(messages)
          results.append({'original':original,'mode':'cereal-types','wire':messages,'argv':command})
        finally:
          if child.poll() is None:
            child.kill();child.wait(timeout=5)
      Path(endpoint[6:]).unlink(missing_ok=True)
    assert typed[0]==typed[1],typed
  (output/'result.json').write_text(json.dumps({'result':'PASS','scenarios':results,
      'binary_sha256':hashlib.sha256((binary/'stats_fork').read_bytes()).hexdigest()},indent=2))
  print('8 original/native fork/drop/backpressure/cereal-type scenarios PASS')


if __name__=='__main__':
  main()
