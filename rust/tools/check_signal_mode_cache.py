"""Actual source shared mode cache, including constructor priming and early flips."""
import argparse
import json
from pathlib import Path
from types import SimpleNamespace
from signal_source import classes


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  cases, expected = [], []
  for initial in (0, 1, 2, 3, -1):
    for changed in (0, 1, 2, 3, -1):
      now, value, reads = [100.], [initial], [0]
      def get_int(_):
        reads[0] += 1
        return value[0]
      scope = classes('openpilot/selfdrive/carrot/signal_mode.py', ['read_mode', 'signal_mode'])
      scope.update(time=SimpleNamespace(monotonic=lambda: now[0]), _params=SimpleNamespace(get_int=get_int), _mode=0, _next_read=0., PARAM='SignalAssistMode')
      constructed = scope['signal_mode']()
      frames, results = [], []
      for offset in (0., .2, .499999, .5, .6, 1., 1.5):
        now[0], value[0] = 100. + offset, changed
        frames.append(dict(now=now[0], value=changed))
        results.append(dict(mode=scope['signal_mode'](), reads=reads[0]))
      cases.append(dict(now=100., initial=initial, frames=frames))
      expected.append(dict(constructed=constructed, frames=results))
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  print(json.dumps(dict(cases=len(cases), reads=sum(len(v['frames']) for v in expected), native_compared=False)))


if __name__ == '__main__':
  main()
