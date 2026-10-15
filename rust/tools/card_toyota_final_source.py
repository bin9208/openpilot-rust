"""Install the pinned Toyota reference modules for isolated development oracles."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
PIN='e8e152dbcfd9574a1791ed12026d1880e87689b3'
ROOT=Path(__file__).resolve().parents[2]
PREFIX='opendbc_repo/opendbc/car/toyota/'
def install(root:Path)->dict:
    previous = sys.modules.get('opendbc.car.toyota')
    if getattr(previous, '__oracle_pin__', None) == PIN:
        return previous.__oracle_report__
    if any(name.startswith('opendbc.car.toyota.') for name in sys.modules):
        raise RuntimeError('Toyota source modules must be selected before loading the baseline package')
    package=root/'pinned-toyota';package.mkdir(parents=True,exist_ok=True)
    hashes={}
    for name in subprocess.check_output(['git','ls-tree','-r','--name-only',PIN,PREFIX],cwd=ROOT,text=True).splitlines():
        if not name.endswith('.py') and not name.endswith('/LICENSE'):continue
        data=subprocess.check_output(['git','show',f'{PIN}:{name}'],cwd=ROOT);target=package/name.removeprefix(PREFIX)
        target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data);hashes[name]=hashlib.sha256(data).hexdigest()
    assert hashes, 'pinned source package must not be empty'
    import opendbc.car.toyota
    opendbc.car.toyota.__path__=[str(package.resolve())]
    result=dict(source_commit=PIN,files=hashes,runtime_python=False,boundary='isolated Toyota source oracle only')
    opendbc.car.toyota.__oracle_pin__ = PIN
    opendbc.car.toyota.__oracle_report__ = result
    (root/'toyota-source-manifest.json').write_text(json.dumps(result,indent=2)+'\n')
    return result
