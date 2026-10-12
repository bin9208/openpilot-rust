from __future__ import annotations

import dataclasses
import hashlib
import json
from pathlib import Path
import sys


def expression(value) -> tuple[str, str]:
  if isinstance(value, bool):
    return 'bool', str(value).lower()
  if isinstance(value, int):
    return 'i64', f'{value:_}'
  if isinstance(value, float):
    return 'f64', repr(value)
  if isinstance(value, str):
    return '&str', json.dumps(value)
  if isinstance(value, tuple) and len(value) in (3, 4):
    return f'[u8; {len(value)}]', '[' + ','.join(map(str, value)) + ']'
  raise ValueError(f'unsupported configuration constant: {value!r}')


def main() -> None:
  root = Path(__file__).resolve().parents[2]
  sys.path.insert(0, str(root / 'openpilot/selfdrive/carrot/cluster'))
  import cluster_config as source

  directory = root / 'rust/crates/cluster/src/config'
  directory.mkdir(exist_ok=True)
  constants = {key: value for key, value in vars(source).items() if key.isupper() and not dataclasses.is_dataclass(value)}
  colors, policy, geometry = [], [], []
  for key, value in constants.items():
    rust_type, literal = expression(value)
    target = colors if isinstance(value, tuple) else geometry if isinstance(value, float) and not key.startswith('CLUSTER_') else policy
    target.append(f'pub const {key}: {rust_type} = {literal};')
  for name, lines in [('colors', colors), ('constants', policy), ('geometry', geometry)]:
    (directory / (name + '.rs')).write_text('\n'.join(lines) + '\n')
  fields = dataclasses.fields(source.ClusterTheme)
  declarations = ['#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]', 'pub struct ClusterTheme {']
  for field in fields:
    value = getattr(source.LIGHT_CLUSTER_THEME, field.name)
    rust_type = 'String' if field.name == 'name' else expression(value)[0]
    declarations.append(f'pub {field.name}: {rust_type},')
  declarations.append('}')
  for name, theme in [('light', source.LIGHT_CLUSTER_THEME), ('dark', source.DARK_CLUSTER_THEME)]:
    declarations.extend(['#[must_use]', f'pub fn {name}_theme() -> ClusterTheme {{', 'ClusterTheme {'])
    for field in fields:
      value = getattr(theme, field.name)
      literal = expression(value)[1] + '.into()' if field.name == 'name' else expression(value)[1]
      declarations.append(f'{field.name}: {literal},')
    declarations.extend(['}', '}'])
  (directory / 'theme.rs').write_text('\n'.join(declarations) + '\n')
  receipt = {
    'source_sha256': hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
    'constants': len(constants),
    'theme_fields': len(fields),
    'names': list(constants),
  }
  (root / '.omo/evidence/253-cluster-resume/config-generation.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
