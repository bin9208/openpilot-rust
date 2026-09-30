"""Actual common/i2c.py with only its device pathname redirected to a fixture."""

import ctypes
import fcntl
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]


def main():
  config=json.loads(Path(sys.argv[1]).read_text())
  device=Path(config['device'])
  assert str(device)==os.environ['AMP_DEVICE_PATH']
  def open_device(path,flags):
    assert path=='/dev/i2c-0'
    return os.open(device,flags)
  namespace={'os':SimpleNamespace(open=open_device,close=os.close,O_RDWR=os.O_RDWR),'fcntl':fcntl,'ctypes':ctypes}
  source=ROOT/'openpilot/common/i2c.py'
  import ast
  tree=ast.parse(source.read_text())
  tree.body=[node for node in tree.body if not isinstance(node,ast.Import)]
  exec(compile(tree,str(source),'exec'),namespace)
  values=[]
  error=None
  try:
    with namespace['SMBus'](0) as bus:
      for action in config['actions']:
        if action['kind']=='read':
          values.append(bus.read_byte_data(16,action['register'],force=True))
        else:
          bus.write_byte_data(16,action['register'],action['value'],force=True)
  except OSError as exception:
    error=exception.errno
  print(json.dumps({'values':values,'errno':error}))


if __name__=='__main__':
  main()
