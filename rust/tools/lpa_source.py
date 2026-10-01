"""Actual-source LPA adapter; only owned serial/reset/lock/CA boundaries are replaced."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys


def load(config, ca):
  path = Path(__file__).resolve().parents[2] / 'openpilot/system/hardware/tici/lpa.py'
  spec = importlib.util.spec_from_file_location('lpa_source', path)
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  module.DEFAULT_DEVICE = config['device']
  module.DEFAULT_TIMEOUT = config['timeout_ms'] / 1000
  module.LOCK_FILE = config['lock']
  module.GSMA_CI_BUNDLE = ca
  module.HTTP_TIMEOUT = 2
  original_run = subprocess.run

  class ProcessBoundary:
    @staticmethod
    def run(argv, **kwargs):
      assert argv == ['/usr/comma/lte/lte.sh', 'start']
      return original_run([config['reset'], 'start'], **kwargs)

  module.subprocess = ProcessBoundary
  return module, module.TiciLPA()


def execute(module, lpa, operation):
  match operation['op']:
    case 'list':
      return [vars(profile) for profile in lpa.list_profiles()]
    case 'active':
      return lpa.get_active_profile()
    case 'delete':
      return lpa.delete_profile(operation['iccid'])
    case 'nickname':
      return lpa.nickname_profile(operation['iccid'], operation['nickname'])
    case 'switch':
      return lpa.switch_profile(operation['iccid'])
    case 'download':
      return lpa.download_profile(operation['activation'], operation.get('nickname'))
    case 'notifications':
      return lpa.process_notifications()
    case 'is_euicc':
      return lpa.is_euicc()
    case 'query':
      return lpa._client.query(operation['command'])
    case 'apdu':
      data, a, b = lpa._client.send_apdu(bytes.fromhex(operation['data']))
      return [data.hex().upper(), a, b]
    case 'command':
      return module.es10x_command(lpa._client, bytes.fromhex(operation['data'])).hex().upper()
    case 'prepare':
      return module.prepare_download(lpa._client, operation['signed'], operation['signature'], operation['certificate'], operation.get('cc'))
    case 'http':
      return module.es9p_request(operation['address'], 'fixture', {})
    case 'codec':
      data = bytes.fromhex(operation['data'])
      return {
        'tlv': [[tag, value.hex().upper(), start, end] for tag, value, start, end in module.iter_tlv(data, with_positions=True)],
        'tbcd': module.string_to_tbcd(operation['digits']).hex().upper(),
        'digits': module.tbcd_to_string(module.string_to_tbcd(operation['digits'])),
        'b64': module.b64e(data),
        'activation': module.parse_lpa_activation_code(operation['activation']),
        'bpp': [part.hex().upper() for part in module._split_bpp(data)],
      }
    case _:
      raise ValueError(operation)


def main():
  request = json.load(sys.stdin)
  module, lpa = load(request['config'], request['ca'])
  rows = []
  for operation in request['operations']:
    try:
      rows.append({'result': execute(module, lpa, operation)})
    except Exception as error:
      rows.append({'error': str(error)})
  lpa._client.close()
  json.dump(rows, sys.stdout)


if __name__ == '__main__':
  main()
