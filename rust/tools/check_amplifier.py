"""Unchanged MAX98089 source policy versus native traces; no hardware operations."""

import argparse
import ast
import hashlib
import json
from pathlib import Path
import random
import subprocess
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'openpilot/system/hardware/tici/amplifier.py'


def definitions():
  tree = ast.parse(SOURCE.read_text())
  tree.body = [node for node in tree.body if not (isinstance(node, ast.Import) and any(alias.name == 'time' for alias in node.names))
               and not (isinstance(node, ast.ImportFrom) and node.module == 'openpilot.common.i2c')]
  return compile(tree, str(SOURCE), 'exec')


CODE = definitions()


def source(config):
  registers = list(config['registers'])
  events = []
  def record(kind, **values):
    index = len(events)
    events.append(dict(kind=kind, **values))
    if index in config.get('fail_events', []) or kind in config.get('fail_kinds', []):
      raise OSError(f'{kind}@{index}')
  class SMBus:
    def __init__(self, bus):
      assert bus == 0
      record('open')
    def __enter__(self):
      return self
    def __exit__(self, *args):
      record('close')
    def read_byte_data(self, address, register, force):
      assert address == 16 and force is True
      record('read', register=register)
      return registers[register]
    def write_byte_data(self, address, register, value, force):
      assert address == 16 and force is True
      record('write', register=register, value=value)
      registers[register] = value
  scope = {'__name__':'amplifier_source_oracle', 'SMBus':SMBus, 'time':SimpleNamespace(sleep=lambda seconds: record('sleep',seconds=seconds)),
           'print':lambda text: record('print',text=text)}
  exec(CODE, scope)
  amplifier = scope['Amplifier'](config['debug'])
  try:
    value = amplifier.initialize_configuration(config['model']) if config['operation']=='initialize' else amplifier.set_global_shutdown(config['disabled'])
    assert type(value) is bool
    outcome = {'value':value}
  except KeyError:
    outcome = {'error':'unknown_model'}
  except OSError as error:
    outcome = {'error':'io','detail':str(error)}
  return {'outcome':outcome,'events':events,'registers':registers}


def cases():
  operations=[{'operation':'initialize','model':model} for model in ['tici','tizi']]
  operations += [{'operation':'shutdown','disabled':disabled} for disabled in [False,True]]
  for op_index, operation in enumerate(operations):
    for byte in range(256):
      yield f'mask-{op_index}-{byte}',dict(operation,debug=False,registers=[byte]*256)
    for debug in [False,True]:
      config=dict(operation,debug=debug,registers=[(index*73+51)%256 for index in range(256)])
      yield f'ordered-{op_index}-{debug}',config
      baseline=source(config)
      for event in range(len(baseline['events'])):
        yield f'fail-once-{op_index}-{debug}-{event}',dict(config,fail_events=[event])
      for kind in ['open','read','write','close','sleep','print']:
        yield f'fail-kind-{op_index}-{debug}-{kind}',dict(config,fail_kinds=[kind],fail_events=[0] if kind=='sleep' or (kind=='print' and not debug) else [])
      yield f'close-overrides-operation-{op_index}-{debug}',dict(config,fail_events=[1,2])
      yield f'retry-sleep-error-{op_index}-{debug}',dict(config,fail_kinds=['read','sleep'])
      yield f'retry-print-error-{op_index}-{debug}',dict(config,fail_kinds=['read'],fail_events=[5 if debug else 4])
  for model in ['mici','TICI','','unknown']:
    yield 'unknown-'+model,dict(operation='initialize',model=model,debug=True,registers=[85]*256)
  for seed in range(4):
    yield f'random-registers-{seed}',dict(operation='initialize',model='tici' if seed%2 else 'tizi',debug=True,registers=list(random.Random(seed).randbytes(256)))


def main():
  parser=argparse.ArgumentParser()
  parser.add_argument('binary',type=Path)
  parser.add_argument('output',type=Path)
  parser.add_argument('--runner',action='append',default=[])
  args=parser.parse_args()
  args.output.mkdir(parents=True)
  inputs=list(cases())
  command=[*args.runner,str(args.binary.resolve())]
  run=subprocess.run(command,input=''.join(json.dumps(config)+'\n' for _,config in inputs),capture_output=True,text=True,check=True)
  native=[json.loads(line) for line in run.stdout.splitlines()]
  assert len(native)==len(inputs)
  report=[]
  for (name,config),actual in zip(inputs,native,strict=True):
    expected=source(config)
    path=args.output/name
    path.mkdir()
    for filename,value in [('input.json',config),('source.json',expected),('native.json',actual)]:
      (path/filename).write_text(json.dumps(value,indent=2))
    assert expected==actual,(name,expected,actual)
    assert type(actual['outcome'].get('value',False)) is bool
    report.append({'scenario':name,'result':'PASS','events':len(actual['events'])})
  (args.output/'result.json').write_text(json.dumps({'result':'PASS','cases':report,'argv':command,'exit':run.returncode,
    'binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'source_sha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest()},indent=2))
  print(len(report),'full-source register/mask/order/debug/failure cases PASS')


if __name__=='__main__':
  main()
