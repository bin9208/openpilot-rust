from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
from card_qa.ci import ROOT, TOOLS, hashes, require_space, run, source_command

GENERATORS = (
  ('source', False), ('wire_source', True), ('acc_source', True), ('state_source', True),
  ('controller_source', False), ('startup_source', False), ('edges_source', False), ('navi_source', False),
)


def stage(fixtures: Path, dbc: Path) -> None:
  fixtures, dbc = fixtures.resolve(), dbc.resolve()
  require_space(fixtures, 1024**3)
  fixtures.mkdir(parents=True)
  dbc.parent.mkdir(parents=True, exist_ok=True)
  commands = fixtures / 'commands'
  run([sys.executable, str(TOOLS / 'card_prepare_assets.py'), str(ROOT), str(dbc)], commands, 'assets')
  sources = [TOOLS / 'card_prepare_assets.py', Path(__file__).resolve(), TOOLS / 'card_qa/ci.py']
  for suffix, needs_dbc in GENERATORS:
    script = TOOLS / f'card_hyundai_{suffix}.py'
    sources.append(script)
    arguments = [str(fixtures)] + ([str(dbc)] if needs_dbc else [])
    run(source_command(script, arguments, dbc), commands, suffix)
  sources.extend((ROOT / 'opendbc_repo/opendbc/car/hyundai').glob('*.py'))
  sources.extend([ROOT / 'opendbc_repo/opendbc/car/car.capnp', ROOT / 'opendbc_repo/opendbc/car/interfaces.py'])
  (fixtures / 'provenance.json').write_text(json.dumps({'source_sha256': hashes(sources),
    'fixtures': str(fixtures), 'dbc': str(dbc), 'generators': [suffix for suffix, _ in GENERATORS]}, indent=2) + '\n')


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--fixtures', type=Path, required=True)
  parser.add_argument('--dbc', type=Path, required=True)
  arguments = parser.parse_args()
  stage(arguments.fixtures, arguments.dbc)


if __name__ == '__main__':
  main()
