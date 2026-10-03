import argparse
from datetime import datetime, timedelta, UTC
import json
import os
from pathlib import Path
from types import SimpleNamespace

import jwt

from original_params_binding import load
from registration_source import definitions


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  persist = json.loads(input())
  root = Path(__file__).resolve().parents[2]
  binding, swaglog = load(args.binding.resolve(), 'ipc:///tmp/logmessage' + os.environ['OPENPILOT_PREFIX'], args.output / 'logs')
  from openpilot.selfdrive.navd import helpers
  api = {'os': os, 'jwt': jwt, 'datetime': datetime, 'timedelta': timedelta, 'UTC': UTC,
         'Paths': SimpleNamespace(persist_root=lambda: persist)}
  exec(definitions(root / 'openpilot/common/api.py', {'KEYS', 'get_key_pair', 'Api'}), api)
  scope = {'os': os, 'Params': lambda: binding.Params(os.environ['PARAMS_ROOT']), 'Api': api['Api'],
           'cloudlog': swaglog.cloudlog, 'coordinate_from_param': helpers.coordinate_from_param}
  exec(definitions(root / 'openpilot/selfdrive/navd/navd.py', {'RouteEngine'}), scope)
  try:
    engine = scope['RouteEngine'](None, None)
    result = {'ok': True, 'host': engine.mapbox_host, 'token': engine.mapbox_token}
  except Exception as error:
    result = {'ok': False, 'error': repr(error)}
  print(json.dumps(result))


if __name__ == '__main__':
  main()
