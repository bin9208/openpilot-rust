import hashlib
import json
from pathlib import Path

from opendbc.dbc.generator.generator import generate_all


def main() -> None:
  root = Path(__file__).resolve().parents[2] / 'opendbc_repo/opendbc/dbc'
  generated = generate_all()
  added = {}
  for name, contents in generated.items():
    target = root / (name + '.dbc')
    if target.exists():
      continue
    with target.open('x', encoding='utf-8') as stream:
      stream.write(contents)
    added[target.name] = hashlib.sha256(contents.encode()).hexdigest()
  print(json.dumps({'added': added, 'existing_files': 'preserved'}))


if __name__ == '__main__':
  main()
