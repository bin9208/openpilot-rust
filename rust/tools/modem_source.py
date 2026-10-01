"""Unchanged-source adapter; substitutes only owned filesystem/executable boundaries."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def load(config):
  path = Path(__file__).resolve().parents[2] / 'openpilot/system/hardware/tici/modem.py'
  spec = importlib.util.spec_from_file_location('source_modem', path)
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  module.AT_PORT = config['at_port']
  module.PPP_PORT = config['ppp_port']
  module.STATE_PATH = config['state']
  module.AT_LOCK = config['lock']
  module.PPPD_CMD[0:3] = [config['sudo'], 'pppd', config['ppp_port']]
  module.STATE_WAIT = config['state_wait_ms'] / 1000
  module.ICCID_CHECK_INTERVAL = config['iccid_interval']
  source_run = subprocess.run

  def run_command(argv, **kwargs):
    argv = list(argv)
    if argv[0] == 'sudo':
      argv[0] = config['sudo']
    elif argv[0] == 'ip':
      argv[0] = config['ip']
    return source_run(argv, **kwargs)

  class Processes:
    Popen = subprocess.Popen
    DEVNULL = subprocess.DEVNULL
    run = staticmethod(run_command)

  module.subprocess = Processes
  source_serial = module.serial.Serial

  class SerialBoundary:
    @staticmethod
    def Serial(port, baudrate, timeout):
      return source_serial(port, baudrate, timeout=config['serial_timeout_ms'] / 1000)

  module.serial = SerialBoundary
  original_temp = tempfile.NamedTemporaryFile

  class Temporary:
    @staticmethod
    def NamedTemporaryFile(**kwargs):
      kwargs['dir'] = str(Path(config['state']).parent)
      return original_temp(**kwargs)

  module.tempfile = Temporary

  class FixtureModem(module.Modem):
    @staticmethod
    def _read_param(key):
      path = Path(config['params']) / key
      return path.read_text().strip() if path.exists() else ''

    @staticmethod
    def _has_modem_manager():
      return Path(config['modem_manager']).is_file()

    def _poll_byte_counters(self):
      try:
        return {key: int((Path(config['statistics']) / key).read_text().strip()) for key in ('tx_bytes', 'rx_bytes')}
      except (OSError, ValueError):
        return {}

  return module, FixtureModem()


def main():
  request = json.load(sys.stdin)
  module, modem = load(request['config'])
  rows = []
  for op in request['operations']:
    result = None
    match op['op']:
      case 'step':
        result = getattr(modem, '_do_' + op['state'].lower())().value
      case 'check':
        modem._check_iccid(module.State(op['state']))
      case 'poll':
        modem._poll()
      case 'identity':
        modem.S.update(modem._read_identity())
      case 'at':
        result = modem._at(op['command'])
      case 'routes':
        result = modem._ppp.maybe_install_routes(op['ip'], op['peer'])
      case 'dns':
        result = modem._ppp.maybe_install_dns(op['servers'])
      case 'sleep':
        time.sleep(op['ms'] / 1000)
      case 'wait_exit':
        deadline = time.monotonic() + 5
        while not modem._ppp.has_exited():
          if time.monotonic() >= deadline:
            raise TimeoutError('PPP fixture exit timeout')
          time.sleep(0.01)
      case 'kill':
        modem._ppp.kill()
      case _:
        raise ValueError(op)
    rows.append({'result': result, 'snapshot': modem.S.copy(), 'fails': modem._ppp.fails})
  json.dump(rows, sys.stdout)


if __name__ == '__main__':
  main()
