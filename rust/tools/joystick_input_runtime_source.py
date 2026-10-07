#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["pytest", "numpy", "pycapnp", "inputs==0.5"]
# ///
from __future__ import annotations

import argparse
from pathlib import Path
import runpy
import sys

from joystickd_source import load_binding


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--keyboard', action='store_true')
  parser.add_argument('--input', type=Path)
  parser.add_argument('--managed', action='store_true')
  args = parser.parse_args()
  load_binding(args.binding.resolve())
  if args.input is not None:
    import inputs
    class OwnedGamepad(inputs.GamePad):
      def _set_name(self):
        self.name = 'Owned gamepad fixture'
    inputs.devices.gamepads = [OwnedGamepad(inputs.devices,
      '/dev/input/by-id/usb-owned-event-joystick', str(args.input))]
  source = Path(__file__).resolve().parents[2] / 'openpilot/tools/joystick/joystick_control.py'
  sys.argv = [str(source), *(['--keyboard'] if args.keyboard else [])]
  try:
    if args.managed:
      from openpilot.tools.joystick import joystick_control
      joystick_control.main()
    else:
      runpy.run_path(str(source), run_name='__main__')
  except KeyboardInterrupt:
    return


if __name__ == '__main__':
  main()
