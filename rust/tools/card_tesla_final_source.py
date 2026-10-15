"""Install the pinned Tesla reference modules for isolated development oracles."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
PIN='8e7eec049241b6ba384e9197aacdfb01f7d88ed3'
ROOT=Path(__file__).resolve().parents[2]
PREFIX='opendbc_repo/opendbc/car/tesla/'
def install(root:Path)->dict:
    previous = sys.modules.get('opendbc.car.tesla')
    if getattr(previous, '__oracle_pin__', None) == PIN:
        return previous.__oracle_report__
    if any(name.startswith('opendbc.car.tesla.') for name in sys.modules):
        raise RuntimeError('Tesla source modules must be selected before loading the baseline package')
    package=root/'pinned-tesla';package.mkdir(parents=True,exist_ok=True)
    hashes={}
    for name in subprocess.check_output(['git','ls-tree','-r','--name-only',PIN,PREFIX],cwd=ROOT,text=True).splitlines():
        if not name.endswith('.py') and not name.endswith('/LICENSE'):continue
        data=subprocess.check_output(['git','show',f'{PIN}:{name}'],cwd=ROOT);target=package/name.removeprefix(PREFIX)
        target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data);hashes[name]=hashlib.sha256(data).hexdigest()
    assert hashes, 'pinned source package must not be empty'
    import opendbc.car.tesla
    opendbc.car.tesla.__path__=[str(package.resolve())]
    result=dict(source_commit=PIN,files=hashes,runtime_python=False,boundary='isolated Tesla source oracle only')
    opendbc.car.tesla.__oracle_pin__ = PIN
    opendbc.car.tesla.__oracle_report__ = result
    (root/'tesla-source-manifest.json').write_text(json.dumps(result,indent=2)+'\n')
    return result
