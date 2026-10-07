from __future__ import annotations

import argparse
import json
from pathlib import Path
import time

from pytest import MonkeyPatch

from card_runtime_source import load_binding


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--params-root', type=Path, required=True)
  parser.add_argument('--dbc', type=Path, required=True)
  parser.add_argument('--steps', type=int)
  parser.add_argument('--fixture-constructor-ready', type=Path)
  parser.add_argument('--fixture-constructor-start', type=Path)
  args = parser.parse_args()
  if args.steps is not None and args.steps <= 0:
    parser.error('--steps must be positive')
  bounded = args.steps is not None and args.steps > 0
  barrier = args.fixture_constructor_ready is not None or args.fixture_constructor_start is not None
  if barrier and not (bounded and args.fixture_constructor_ready is not None and args.fixture_constructor_start is not None
                      and args.fixture_constructor_ready != args.fixture_constructor_start):
    parser.error('constructor fixture requires distinct ready/start paths and --steps')
  load_binding(args.binding)
  from openpilot.common.params import Params
  from openpilot.selfdrive.carrot.radar import radarcan
  from opendbc.car.hyundai import radar_interface, hyundaicanfd
  from opendbc.car.car_helpers import interfaces

  def parameters():
    return Params(str(args.params_root))

  with MonkeyPatch.context() as fixture:
    fixture.setattr(radarcan, 'Params', parameters)
    fixture.setattr(radar_interface, 'Params', parameters)
    fixture.setattr(hyundaicanfd, 'Params', parameters)
    fixture.setattr('opendbc.can.dbc.DBC_PATH', str(args.dbc.resolve()))
    fixture.setattr(radar_interface, 'DBC_PATH', str(args.dbc.resolve()))
    if bounded:
      class BoundedPoller(radarcan.messaging.Poller):
        def __init__(self):
          super().__init__()
          self.remaining = args.steps

        def poll(self, timeout):
          if self.remaining == 0:
            raise TimeoutError('bounded source radarcan poll limit')
          self.remaining -= 1
          return super().poll(timeout)

      fixture.setattr(radarcan.messaging, 'Poller', BoundedPoller)
    if barrier:
      pending = True

      def record(value):
        path = args.fixture_constructor_ready.with_suffix('.pending')
        path.write_text(json.dumps(value))
        path.replace(args.fixture_constructor_ready)

      def constructor(original):
        def observed(cp):
          nonlocal pending
          if not pending:
            return original(cp)
          started = time.monotonic_ns()
          try:
            state = original(cp)
          except (ValueError, KeyError, FileNotFoundError, AssertionError) as error:
            record({'status': 'error', 'constructor_started_ns': started,
              'constructor_completed_ns': time.monotonic_ns(), 'error': {'kind': type(error).__name__, 'message': str(error)}})
            raise
          record({'status': 'ready', 'constructor_started_ns': started,
            'constructor_completed_ns': time.monotonic_ns(), 'error': None})
          deadline = time.monotonic() + 20
          while not args.fixture_constructor_start.exists():
            if time.monotonic() >= deadline:
              raise TimeoutError('bounded source constructor fixture was not released within20s')
            time.sleep(.001)
          pending = False
          return state
        return observed

      for interface in set(interfaces.values()):
        fixture.setattr(interface, 'RadarInterface', constructor(interface.RadarInterface))
    radarcan.main()


if __name__ == '__main__':
  main()
