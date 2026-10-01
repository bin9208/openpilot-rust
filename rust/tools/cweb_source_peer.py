import json
import os
from pathlib import Path
import sys

from original_params_binding import load


def main():
  binding, root = Path(sys.argv[1]), Path(sys.argv[2])
  params_module, _ = load(binding, 'ipc://' + str(root / 'source-log'), root / 'source-logs')
  params = params_module.Params(str(root / 'source-params'))
  for key, value in json.loads((root / 'values.json').read_text()).items():
    Path(params.get_param_path(key)).write_bytes(bytes(value))
  from openpilot.selfdrive.carrot import cweb_push
  cweb_push.Params = lambda: params
  cweb_push.get_local_ip = lambda iface: cweb_push._usable_ip(Path(os.environ['CWEB_FIXTURE_IP']).read_text())
  sys.argv = ['cweb_push', *sys.argv[3:]]
  cweb_push.main()


if __name__ == '__main__':
  main()
