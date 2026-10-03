import argparse
import json
import os
from pathlib import Path
import runpy
import sys

from original_params_binding import load


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  binding, _ = load(args.binding.resolve(), 'ipc://' + str(args.output / 'log'), args.output / 'logs')
  original = binding.Params
  binding.Params = lambda: original(os.environ['PARAMS_ROOT'])
  source = Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/navd/set_destination.py'
  sys.argv = [str(source), *json.load(sys.stdin)]
  runpy.run_path(str(source), run_name='__main__')


if __name__ == '__main__':
  main()
