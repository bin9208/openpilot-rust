#!/usr/bin/env python3
"""Real original and native ZMQ failure boundaries, including persistence order."""
import argparse
import json
import os
from pathlib import Path
import tempfile

import zmq
from logging_producer_reference import original_socket_handler
from timed_fixtures import environment, native, normalized
from timed_reference import Source


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  reports = []
  for malformed in [False, True]:
    outputs = []
    for implementation in ['python', 'rust']:
      with tempfile.TemporaryDirectory(prefix='timed-log-') as temporary, environment(Path(temporary)) as (config, params):
        actions = [{'kind': 'set_time', 'epoch': 1790000000.0}, {'kind': 'close_logger'},
                   {'kind': 'set_time', 'epoch': 1790000030.0}, {'kind': 'apply', 'zone': 'Asia/Seoul', 'source': 'wifi'}]
        config['actions'] = actions
        if malformed:
          (params / 'TimezoneSource').write_bytes(b'\xff')
        if implementation == 'rust':
          rows, records = native(args.binary, config, args.output / f'native-{malformed}.jsonl')
          results = ['error' if 'error' in row['result'] else 'ok' for row in rows]
        else:
          source = Source(config, params)
          endpoint = 'ipc:///tmp/logmessage' + os.environ['OPENPILOT_PREFIX']
          context = zmq.Context()
          collector = context.socket(zmq.PULL)
          collector.setsockopt(zmq.LINGER, 0)
          collector.bind(endpoint)
          handler = original_socket_handler(endpoint, source.logger)
          source.logger.addHandler(handler)
          results = []
          try:
            for action in actions:
              try:
                if action['kind'] == 'close_logger':
                  handler.sock.close()
                else:
                  source.action(action)
                results.append('ok')
              except zmq.ZMQError as error:
                assert error.errno == zmq.ENOTSOCK, error
                results.append('error')
            records = []
            while collector.poll(50):
              records.append(json.loads(collector.recv()[1:]))
          finally:
            handler.close()
            source.logger.removeHandler(handler)
            collector.close()
            context.term()
          (args.output / f'source-{malformed}.logs.json').write_text(json.dumps(records, indent=2) + '\n')
        assert results == ['ok', 'ok', 'error', 'error'], results
        assert len(records) == 1 and records[0]['msg'] == 'Time diff too small: 0.0s', records
        # The malformed source warning fails before commands; the normal success log fails after writes.
        values = {key: (params / key).read_bytes().hex() if (params / key).exists() else None for key in ['TimezoneName', 'TimezoneSource']}
        if malformed:
          assert values == {'TimezoneName': None, 'TimezoneSource': 'ff'}, values
          (params / 'TimezoneSource').unlink()
        observed = normalized(config, params, [])
        outputs.append({'results': results, 'state': observed, 'raw_params': values})
    assert outputs[0] == outputs[1], outputs
    reports.append(outputs)
  (args.output / 'summary.json').write_text(json.dumps({'passed': True, 'scenarios': 2, 'results': reports}, indent=2) + '\n')
  print('PASS: original/native closed-log propagation and command/Params side-effect boundaries')


if __name__ == '__main__':
  main()
