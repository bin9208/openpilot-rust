import argparse
import copy
import hashlib
import json
from pathlib import Path
import random
import subprocess

from generate_selfdrive_alerts import generate, load_source, serialize_alert

CATEGORIES = ('enable', 'preEnable', 'overrideLateral', 'overrideLongitudinal', 'noEntry',
              'warning', 'userDisable', 'softDisable', 'immediateDisable', 'permanent')


def requests(data):
  rng = random.Random(168)
  rows = []
  alert = {'alert_text_1': 'dynamic', 'alert_text_2': '콜백', 'alert_status': 0, 'alert_size': 2,
           'priority': 'low', 'visual_alert': 0, 'audible_alert': 0, 'duration': 100,
           'creation_delay': .03, 'alert_type': '', 'event_type': None}
  for mici in (False, True):
    entries = data['mici' if mici else 'tici']
    for entry in entries:
      for frame in range(3):
        rows.append({'reset': mici if frame == 0 else None, 'clear': True,
                     'add': [[entry['event'], False], [entry['event'], False]], 'categories': list(CATEGORIES),
                     'callback_alert': alert, 'prefix': ('', 'ko:', 'en:')[frame]})
    for index in range(2_000):
      rows.append({'reset': mici if index % 64 == 0 else None, 'clear': rng.random() < .8,
                   'add': [[rng.choice(entries)['event'], rng.random() < .015] for _ in range(rng.randrange(5))],
                   'categories': rng.choices(CATEGORIES, k=rng.randrange(12)), 'callback_alert': alert,
                   'prefix': rng.choice(('', 'ko:', 'en:'))})
    reverse = next(entry['event'] for entry in entries if entry['name'] == 'reverseGear')
    for index in range(61):
      rows.append({'reset': mici if index == 0 else None, 'clear': True, 'add': [[reverse, False]],
                   'categories': list(CATEGORIES), 'callback_alert': alert, 'prefix': ''})
  return rows


def callback_boundary(context, spec):
  def callback():
    context['calls'].append(spec)
    return context['alert']
  return callback


def original(rows, data):
  results = []
  context = {'calls': [], 'alert': None}
  source, machine = None, None
  for row in rows:
    if row['reset'] is not None:
      device = 'mici' if row['reset'] else 'tici'
      source = load_source(device)
      for entry in data[device]:
        for definition in entry['definitions']:
          detail = definition['definition']
          if detail['kind'] == 'callback':
            source['EVENTS'][entry['event']][definition['category']] = callback_boundary(context, detail['callback'])
      machine = source['Events']()
    current = copy.deepcopy(row['callback_alert'])
    current['priority'] = ('lowest', 'lower', 'low', 'mid', 'high', 'highest').index(current['priority'])
    callback_alert = source['Alert']('', '', 0, 0, 0, 0, 0, 0)
    callback_alert.__dict__.update(current)
    context['alert'] = callback_alert
    source['tr'] = lambda text, prefix=row['prefix']: prefix + text if text else ''
    if row['clear']:
      machine.clear()
    for name, static in row['add']:
      machine.add(name, static)
    context['calls'] = []
    alerts = machine.create_alerts(row['categories'])
    messages = [{'name': event, 'categories': list(source['EVENTS'].get(event, {}))} for event in machine.names]
    results.append({'names': machine.names.copy(), 'counters': {str(k): v for k, v in machine.event_counters.items()},
                    'messages': messages, 'contains': [machine.contains(category) for category in row['categories']],
                    'alerts': [serialize_alert(alert) for alert in alerts], 'callbacks': context['calls']})
  return results


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  data = generate()
  rows = requests(data)
  expected = original(rows, data)
  payload = ''.join(json.dumps(row) + '\n' for row in rows)
  (args.output / 'input.jsonl').write_text(payload)
  result = subprocess.run([str(args.binary.resolve())], input=payload, capture_output=True, text=True, timeout=60)
  (args.output / 'native.jsonl').write_text(result.stdout)
  (args.output / 'native.stderr').write_text(result.stderr)
  (args.output / 'source.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in expected))
  assert result.returncode == 0, result.stderr
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  assert len(actual) == len(expected)
  for index, (source, native) in enumerate(zip(expected, actual, strict=True)):
    assert source == native, (index, source, native)
  report = {'result': 'PASS', 'steps': len(rows), 'sources': data['sources'],
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
            'scope': 'event storage, ordering, translation, callback dispatch and delays; callback bodies are separate'}
  (args.output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
