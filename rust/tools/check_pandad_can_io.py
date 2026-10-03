import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess

from pandad_protocol_cases import packing, seeds


def sent(row, result=0):
  return {'offset': row['offset'], 'write_result': result, 'replies': [],
          'operations': [{'op': 'send', 'frames': row['frames']}]}


def received(chunks, offset=0):
  return {'offset': offset, 'write_result': 0, 'replies': chunks,
          'operations': [{'op': 'receive'} for _ in chunks]}


def reply(data, healthy=True, count=None):
  return {'data': data, 'healthy': healthy, 'count': len(data) if count is None else count}


def cases(packets, maxout):
  rows = [sent(row, (-7, 0, 3, 4096)[index % 4]) for index, row in enumerate(packing())]
  for packet in packets:
    for split in sorted({0, 1, 5, 6, len(packet) - 1, len(packet)}):
      rows.append(received([reply(packet[:split]), reply([255] * 7, False), reply(packet[split:])], 4))
      rows.append(received([reply(packet[:split]), reply([], count=0), reply(packet[split:])], 8))
    for bit in range(8):
      corrupted = packet.copy()
      corrupted[5] ^= 1 << bit
      rows.append(received([reply(packets[0] + corrupted + packets[-1]), reply(packet)]))
  rng = random.Random(17503)
  for _ in range(100):
    stream = [byte for packet in rng.choices(packets, k=rng.randrange(1, 20)) for byte in packet]
    chunks = []
    while stream:
      size = rng.randrange(1, 128)
      chunks.append(reply(stream[:size]))
      stream = stream[size:]
    rows.append(received(chunks, rng.choice((0, 4, 8, 2**32 - 1))))
  full = packets[-1] * 235
  rows.append(received([reply(full[:16384]), reply(full[16384:])]))
  for count in (-1, -4, -7):
    rows.append(received([reply([], False, count), reply(packets[0])]))
    if not maxout:
      rows.append(received([reply(packets[-1][:4]), reply([], True, count), reply(packets[-1][4:])]))
  return rows


def run(command, rows, environment, output, label):
  payload = ''.join(json.dumps(row) + '\n' for row in rows)
  (output / f'{label}.input.jsonl').write_text(payload)
  result = subprocess.run(command, input=payload, text=True, capture_output=True, env=environment, timeout=90)
  (output / f'{label}.jsonl').write_text(result.stdout)
  (output / f'{label}.stderr').write_text(result.stderr)
  result.check_returncode()
  values = [json.loads(line) for line in result.stdout.splitlines()]
  assert len(values) == len(rows), (label, len(values), len(rows))
  assert all(value['unused_replies'] == 0 for value in values)
  return values


def main():
  parser = argparse.ArgumentParser(description='Compare original Panda bulk CAN methods with recorded I/O.')
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--native', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--runner', nargs=argparse.REMAINDER, default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  source_command = [str(args.source.resolve())]
  native_command = [*args.runner, str(args.native.resolve())]
  environment = dict(os.environ)
  environment.pop('PANDAD_MAXOUT', None)
  encoded = run(source_command, [sent(row) for row in seeds()], environment, args.output, 'seeds')
  packets = [row['results'][0]['calls'][0]['data'] for row in encoded]
  summaries = []
  for maxout in (False, True):
    if maxout:
      environment['PANDAD_MAXOUT'] = '1'
    output = args.output / ('maxout' if maxout else 'normal')
    output.mkdir()
    corpus = cases(packets, maxout)
    source = run(source_command, corpus, environment, output, 'source')
    native = run(native_command, corpus, environment, output, 'native')
    for index, (expected, actual) in enumerate(zip(source, native, strict=True)):
      if expected != actual:
        (output / 'mismatch.json').write_text(json.dumps({'index': index, 'input': corpus[index],
                                                        'source': expected, 'native': actual}, indent=2))
        raise AssertionError(f'CAN I/O differs at {index}, maxout={maxout}')
    summaries.append({'maxout': maxout, 'scenarios': len(corpus),
                      'operations': sum(len(row['operations']) for row in corpus)})
  report = {'result': 'PASS', 'runs': summaries,
            'source_sha256': hashlib.sha256(args.source.read_bytes()).hexdigest(),
            'native_sha256': hashlib.sha256(args.native.read_bytes()).hexdigest(),
            'native_command': native_command,
            'scope': 'exact bulk/control ordering, endpoint, timeout, bytes, partial frames, health gate and checksum reset',
            'excluded_source_invalid_contract': 'healthy negative receive count with MAXOUT would exceed its fixed junk buffer'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
