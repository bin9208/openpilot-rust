from __future__ import annotations

import ast
import dataclasses
import hashlib
import inspect
import json
from pathlib import Path
import re
import sys
import typing

from cluster_model_types import ENUMS, default_record, expression, rust_type


def standard_default(value) -> bool:
  if value is None:
    return True
  if dataclasses.is_dataclass(value):
    return dataclasses.asdict(value) == dataclasses.asdict(default_record(type(value)))
  if isinstance(value, tuple):
    return all(standard_default(item) for item in value)
  return value in (False, 0, '', b'')


def main() -> None:
  root = Path(__file__).resolve().parents[2]
  sys.path.insert(0, str(root / 'openpilot/selfdrive/carrot/cluster'))
  import cluster_config as config
  import cluster_models as source

  live_path = root / 'openpilot/selfdrive/carrot/cluster/cluster_live.py'
  standby = next(node for node in ast.parse(live_path.read_text()).body if isinstance(node, ast.FunctionDef) and node.name == 'standby_state')
  scope = {**vars(source), **vars(config)}
  exec(compile(ast.Module(body=[standby], type_ignores=[]), str(live_path), 'exec'), scope)
  records = [cls for cls in vars(source).values() if inspect.isclass(cls) and cls.__module__ == 'cluster_models' and dataclasses.is_dataclass(cls)]
  directory = root / 'rust/crates/cluster/src/models'
  directory.mkdir(exist_ok=True)
  modules, inventories, defaults, rust_defaults = [], [], {}, []
  for cls in records:
    name = re.sub(r'(?<!^)(?=[A-Z])', '_', cls.__name__).lower()
    values = scope['standby_state']() if cls.__name__ == 'ClusterUiState' else default_record(cls)
    defaults[cls.__name__] = dataclasses.asdict(values)
    hints = typing.get_type_hints(cls)
    fields = dataclasses.fields(cls)
    derive = (
      'Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize'
      if cls.__name__ == 'ClusterAlert'
      else 'Clone, Debug, serde::Serialize, serde::Deserialize'
      if cls.__name__ in ('ClusterUiState', 'NaviMediaFrame', 'NaviDashboardState')
      else 'Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize'
    )
    derived_default = all(typing.get_origin(hints[field.name]) is not typing.Literal and standard_default(getattr(values, field.name)) for field in fields)
    if derived_default:
      derive += ', Default'
    body = [f'#[derive({derive})]', f'pub struct {cls.__name__} {{']
    if cls.__name__ == 'ClusterUiState':
      body.insert(0, '#[expect(clippy::struct_excessive_bools, reason="Preserves the flat source UI state contract")]')
    for field in fields:
      annotation = hints[field.name]
      field_type = rust_type(annotation)
      if field.name in ('color',):
        field_type = '[u8; 3]'
      if field.name == 'plane_strides':
        field_type = 'Option<[i64; 3]>'
      if cls.__name__ == 'NaviMediaFrame' and field.name == 'data':
        field_type = 'Option<std::sync::Arc<[u8]>>'
      if cls.__name__ == 'NaviMediaFrame' and field.name == 'plane_data':
        field_type = 'Option<[std::sync::Arc<[u8]>; 3]>'
      if (
        (cls.__name__ in ('NaviMediaFrame', 'NaviItemStatus') and field.name == 'sequence')
        or (cls.__name__ == 'NaviItemMeta' and field.name in ('sequence', 'source_timestamp_ms'))
        or (cls.__name__ == 'NaviLiveState' and field.name == 'generation')
      ):
        field_type = 'u64'
      if field.name == 'hardware_buffer':
        body.append('#[serde(default, with="crate::media::frame::optional")]')
      body.append(f'pub {field.name}: {field_type},')
    body.append('}')
    default = [f'impl Default for {cls.__name__} {{', 'fn default() -> Self {', 'Self {']
    default.extend(field.name + ': ' + expression(hints[field.name], getattr(values, field.name)) + ',' for field in fields)
    default.extend(['}', '}', '}'])
    if derived_default:
      default = []
    identifiers = set(re.findall(r'\b[A-Z][A-Za-z0-9_]+\b', '\n'.join(body + default)))
    imports = sorted(identifiers.intersection({record.__name__ for record in records} | set(ENUMS.values())) - {cls.__name__})
    if imports:
      body.insert(0, 'use super::{' + ','.join(imports) + '};')
    if cls.__name__ == 'ClusterUiState':
      default.insert(1, '#[expect(clippy::too_many_lines, reason="Preserves all 148 original standby fields")]')
      sub = directory / name
      sub.mkdir(exist_ok=True)
      body.append('mod defaults;')
      (sub / 'mod.rs').write_text('\n'.join(body) + '\n')
      (sub / 'defaults.rs').write_text(
        'use super::{'
        + ','.join(
          sorted(set(re.findall(r'\b[A-Z][A-Za-z0-9_]+\b', '\n'.join(default))).intersection({record.__name__ for record in records} | set(ENUMS.values())))
        )
        + '};\n'
        + '\n'.join(default)
        + '\n'
      )
    else:
      (directory / (name + '.rs')).write_text('\n'.join(body + default) + '\n')
    modules.extend([f'mod {name};', f'pub use {name}::{cls.__name__};'])
    inventories.append({'class': cls.__name__, 'fields': [field.name for field in fields]})
    rust_defaults.append(f'record::<{cls.__name__}>("{cls.__name__}"),')
  enum_lines = []
  for variants, name in ENUMS.items():
    enum_lines.extend(
      ['#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]', '#[serde(rename_all="lowercase")]', f'pub enum {name} {{']
    )
    enum_lines.extend(variant[:1].upper() + variant[1:] + ',' for variant in variants)
    enum_lines.append('}')
  (directory / 'enums.rs').write_text('\n'.join(enum_lines) + '\n')
  (directory.parent / 'models.rs').write_text('mod radar;\npub use radar::*;\nmod enums;\npub use enums::*;\n' + '\n'.join(modules) + '\n')
  tests = root / 'rust/crates/cluster/tests'
  (tests / 'data/models_defaults.json').write_text(json.dumps(defaults, indent=2) + '\n')
  (tests / 'models_defaults.rs').write_text(
    'use openpilot_cluster::models::*;\n'
    + 'fn record<T: Default + serde::Serialize>(name: &str) -> (String,serde_json::Value) {\n'
    + '(name.to_owned(),serde_json::to_value(T::default()).unwrap())\n}\n'
    + '#[test]\nfn all_source_record_defaults_match() {\nlet actual: serde_json::Map<String,serde_json::Value> = ['
    + '\n'.join(rust_defaults)
    + '].into_iter().collect();\n'
    + 'let expected: serde_json::Value = serde_json::from_str(include_str!("data/models_defaults.json")).unwrap();\n'
    + 'assert_eq!(serde_json::Value::Object(actual), expected);\n}\n'
  )
  evidence = root / '.omo/evidence/253-cluster-resume'
  receipt = {
    'classes': len(records),
    'records': inventories,
    'source_sha256': hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
    'state_fields': len(dataclasses.fields(source.ClusterUiState)),
  }
  (evidence / 'models-generation.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps({'classes': len(records), 'state_fields': receipt['state_fields']}))


if __name__ == '__main__':
  main()
