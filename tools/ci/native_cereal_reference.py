#!/usr/bin/env python3
"""Generate source expectations with the complete original Python schema."""
import hashlib
import json
import os
import sys
import capnp
from pathlib import Path
from openpilot.cereal import log


def main():
  source = Path(os.environ['GITHUB_WORKSPACE'])
  output = Path(os.environ['CI_JOB_TEMP'])
  packets = []
  message = log.Event.new_message(logMonoTime=123456789, valid=True)
  message.init('carState').vEgo = 12.5
  packets.append(message.to_bytes().hex())
  message = log.Event.new_message(logMonoTime=987654321, valid=False)
  message.logMessage = 'SDK path | capnp 원본'
  packets.append(message.to_bytes().hex())
  (output/'native-cereal-original-wire.txt').write_text('\n'.join(packets)+'\n')
  paths = ['openpilot/cereal/__init__.py','openpilot/cereal/log.capnp','openpilot/cereal/custom.capnp','openpilot/cereal/deprecated.capnp',
           'openpilot/cereal/include/c++.capnp','opendbc_repo/opendbc/car/car.capnp']
  extension = Path(sys.modules[capnp.SchemaParser.__module__].__file__)
  evidence = dict(source_commit=os.environ['GITHUB_SHA'],cases=2,full_original_schema=True,
                  python=sys.version,pycapnp_version=capnp.__version__,extension=str(extension),
                  extension_sha256=hashlib.sha256(extension.read_bytes()).hexdigest(),
                  schema_sha256={p:hashlib.sha256((source/p).read_bytes()).hexdigest() for p in paths})
  (output/'native-cereal-original-reference.json').write_text(json.dumps(evidence,indent=2)+'\n')


if __name__ == '__main__':
  main()
