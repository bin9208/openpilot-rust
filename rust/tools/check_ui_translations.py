"""Validate all repository PO catalogs and plural selectors against actual source."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
from types import ModuleType, SimpleNamespace

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root))
params = ModuleType('openpilot.common.params')
sys.modules[params.__name__] = params
logging = ModuleType('openpilot.common.swaglog')
logging.cloudlog = SimpleNamespace(debug=lambda *a: None, error=lambda *a: None)
sys.modules[logging.__name__] = logging
from openpilot.system.ui.lib import multilang as source
expected = {}
for path in sorted((root / 'openpilot/selfdrive/ui/translations').glob('*.po')):
  translations, plurals = source.load_translations(path)
  expected[path.name] = {'translations': translations, 'plurals': plurals}
expected['selectors'] = {lang: [selector(n) for n in range(-220, 221)] for lang, selector in source.PLURAL_SELECTORS.items()}
expected['selectors']['missing'] = [0] * 441
result = subprocess.run([str(args.binary), str(root / 'openpilot/selfdrive/ui/translations')], text=True, capture_output=True, check=True)
(args.output / 'translations-source.json').write_text(json.dumps(expected, indent=2, ensure_ascii=False))
(args.output / 'translations-native.json').write_text(result.stdout)
actual = json.loads(result.stdout)
assert actual == expected
print(f'PASS: {len(expected) - 1} complete source/native PO catalogs and {len(expected["selectors"]) * 441} plural selector cases')
