from __future__ import annotations

import argparse
import hashlib
import importlib
import json
from pathlib import Path
from card_qa.ci import TOOLS, hashes, require_space, run, source_command

BRANDS = ('tesla', 'mazda', 'nissan', 'chrysler', 'rivian', 'ford', 'subaru', 'toyota', 'gm', 'honda', 'volkswagen')


def generate(brand: str, output: Path, dbc: Path, retained: Path | None) -> None:
  from can_source import load
  load()
  import opendbc.can.dbc as source_dbc
  source_dbc.DBC_PATH = str(dbc)
  source_dbc.DBC.cache_clear()
  from card_qa.runtime_catalog import EXTENDED_BRANDS, FIXTURE_NAMES
  if retained is not None:
    cases = json.loads(retained.read_text())
  else:
    module = importlib.import_module(f'card_qa.{brand}.scenarios')
    cases = module.cases('runtime') if brand in ('gm', 'volkswagen') else module.cases()
  selected = []
  candidates = ('TESLA_MODEL_3', 'TESLA_MODEL_Y') if brand == 'tesla' else tuple(
    candidate for candidate, owner in EXTENDED_BRANDS.items() if owner == brand)
  for candidate in candidates:
    matching = [case for case in cases if case['candidate'] == candidate and case['op'] == 'runtime']
    case = next(case for case in matching if case['name'] == FIXTURE_NAMES[candidate]) if candidate in FIXTURE_NAMES else matching[0]
    assert len(case['steps']) >= 81, (candidate, case['name'])
    selected.append({**case, 'steps': case['steps'][:81]})
  output.mkdir(parents=True)
  (output / 'input.json').write_text(json.dumps(selected) + '\n')
  (output / 'selection.json').write_text(json.dumps({'brand': brand,
    'source': str(retained) if retained is not None else f'card_qa.{brand}.scenarios.cases',
    'cases': [{'candidate': case['candidate'], 'name': case['name'], 'steps': len(case['steps'])} for case in selected]}, indent=2) + '\n')


def stage(output: Path, fixtures: Path, dbc: Path, retained: dict[str, Path]) -> None:
  output, fixtures, dbc = output.resolve(), fixtures.resolve(), dbc.resolve()
  require_space(output, 1024**3)
  output.mkdir(parents=True)
  hyundai = output / 'hyundai'
  hyundai.mkdir()
  (hyundai / 'state.json').write_bytes((fixtures / 'state.json').read_bytes())
  for brand in BRANDS:
    arguments = ['--brand', brand, '--output', str(output / brand), '--dbc', str(dbc)]
    if brand in retained:
      arguments += ['--input', str(retained[brand].resolve())]
    run(source_command(Path(__file__).resolve(), arguments, dbc), output / 'commands', brand)
  sources = [Path(__file__).resolve(), TOOLS / 'card_qa/ci.py', TOOLS / 'card_qa/runtime_inputs.py', TOOLS / 'check_card_runtime_pumped.py']
  for brand in BRANDS:
    sources.extend((TOOLS / f'card_qa/{brand}').glob('*.py'))
  (output / 'provenance.json').write_text(json.dumps({'source_sha256': hashes(sources),
    'input_sha256': {str(path.relative_to(output)): hashlib.sha256(path.read_bytes()).hexdigest()
                     for path in output.glob('*/input.json')}, 'hyundai_fixtures': str(fixtures)}, indent=2) + '\n')


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--dbc', type=Path, required=True)
  parser.add_argument('--fixtures', type=Path)
  parser.add_argument('--brand', choices=BRANDS)
  parser.add_argument('--input', type=Path)
  parser.add_argument('--retained', action='append', default=[], metavar='BRAND=INPUT_JSON')
  arguments = parser.parse_args()
  if arguments.brand is not None:
    generate(arguments.brand, arguments.output.resolve(), arguments.dbc.resolve(), arguments.input)
  else:
    if arguments.fixtures is None:
      parser.error('--fixtures is required when staging all brands')
    retained = {}
    for entry in arguments.retained:
      brand, separator, path = entry.partition('=')
      if not separator or brand not in BRANDS or brand in retained:
        parser.error('retained inputs must be unique BRAND=INPUT_JSON entries')
      retained[brand] = Path(path)
    stage(arguments.output, arguments.fixtures, arguments.dbc, retained)


if __name__ == '__main__':
  main()
