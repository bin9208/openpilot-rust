"""Actual final manager-entry main latch after worker return or ordinary failure."""
import argparse
import json
from pathlib import Path
import sys
from types import ModuleType, SimpleNamespace
from signal_source import classes


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  cases, expected = [], []
  for failed in (False, True):
    for waits in range(16):
      actions, requests = [], [True]*waits + [False]
      def worker():
        actions.append(['run', failed])
        if failed:
          raise ValueError('source worker failure')
      def requested():
        value = requests.pop(0)
        actions.append(['requested', value])
        return value
      module = ModuleType('openpilot.common.swaglog')
      module.cloudlog = SimpleNamespace(exception=lambda message: actions.append(['exception', message]))
      sys.modules[module.__name__] = module
      scope = classes('openpilot/selfdrive/modeld/signal_color_shadow.py', ['main'])
      scope.update(run=worker, requested=requested, time=SimpleNamespace(sleep=lambda seconds: actions.append(['sleep', seconds])))
      scope['main']()
      assert not requests
      cases.append(dict(failed=failed, waits=waits))
      expected.append(actions)
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  print(json.dumps(dict(cases=len(cases), native_compared=False)))


if __name__ == '__main__':
  main()
