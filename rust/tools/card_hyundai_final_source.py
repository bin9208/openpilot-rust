"""Install the complete pinned Hyundai package for validation-only source oracles.

Runtime imports never use this module. Generic opendbc providers stay available
from the retained source tree; every Hyundai Python file is read from the pin.
"""
from __future__ import annotations
import hashlib
import json
from pathlib import Path
import subprocess
import sys

PIN = 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
PREFIX = 'opendbc_repo/opendbc/car/hyundai/'


def install(root: Path) -> dict:
    previous = sys.modules.get('opendbc.car.hyundai')
    if getattr(previous, '__oracle_pin__', None) == PIN:
        return previous.__oracle_report__
    loaded = [name for name in sys.modules if name.startswith('opendbc.car.hyundai.')]
    if loaded:
        raise RuntimeError(f'Pinned Hyundai source must be installed before imports: {loaded}')
    import opendbc
    can_loaded = [name for name in sys.modules if name == 'opendbc.can' or name.startswith('opendbc.can.')]
    if can_loaded:
        raise RuntimeError(f'Pinned CAN source must be installed before imports: {can_loaded}')
    can_root = root / 'pinned-can-package'
    can_package = can_root / 'can'
    can_package.mkdir(parents=True, exist_ok=True)
    can_names = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', PIN, 'opendbc_repo/opendbc/can/'], text=True).splitlines()
    can_hashes = {}
    for name in can_names:
        relative = name.removeprefix('opendbc_repo/opendbc/can/')
        if '/' in relative or not relative.endswith('.py'):
            continue
        data = subprocess.check_output(['git', 'show', f'{PIN}:{name}'])
        (can_package / relative).write_bytes(data)
        can_hashes[name] = hashlib.sha256(data).hexdigest()
    opendbc.__path__.insert(0, str(can_root.resolve()))
    package = root / 'pinned-hyundai'
    package.mkdir(parents=True, exist_ok=True)
    names = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', PIN, PREFIX], text=True).splitlines()
    hashes = dict(can_hashes)
    for name in names:
        if not name.endswith('.py'):
            continue
        data = subprocess.check_output(['git', 'show', f'{PIN}:{name}'])
        target = package / name.removeprefix(PREFIX)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        hashes[name] = hashlib.sha256(data).hexdigest()
    import opendbc.car.hyundai
    opendbc.car.hyundai.__path__ = [str(package.resolve())]
    report = dict(source_commit=PIN, files=hashes, runtime_python=False,
                  boundary='Params, deterministic clock and synthetic CAN input only')
    opendbc.car.hyundai.__oracle_pin__ = PIN
    opendbc.car.hyundai.__oracle_report__ = report
    (root / 'hyundai-source-manifest.json').write_text(json.dumps(report, indent=2) + '\n')
    return report
