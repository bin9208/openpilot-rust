from __future__ import annotations

import dataclasses
import json
import types
import typing

ENUMS = {
  ('off', 'paused', 'engaged'): 'CruiseDisplayState',
  ('ok', 'pull', 'missing', 'unknown'): 'GitBranchStatusState',
  ('angle', 'torque'): 'SteeringOutputKind',
  ('fuel', 'battery'): 'EnergyGaugeLabel',
}


def rust_type(value) -> str:
  origin, args = typing.get_origin(value), typing.get_args(value)
  primitive = {float: 'f64', int: 'i64', str: 'String', bool: 'bool', bytes: 'Vec<u8>', object: 'std::sync::Arc<crate::media::HardwareFrame>'}
  if value in primitive:
    return primitive[value]
  if origin in (types.UnionType, typing.Union):
    assert len(args) == 2 and type(None) in args, value
    return f'Option<{rust_type(next(argument for argument in args if argument is not type(None)))}>'
  if origin is typing.Literal:
    return ENUMS[args]
  if origin is tuple:
    if len(args) == 2 and args[1] is Ellipsis:
      return f'Vec<{rust_type(args[0])}>'
    assert len(set(args)) == 1, value
    inner = 'u8' if args == (int, int, int) else rust_type(args[0])
    return f'[{inner}; {len(args)}]'
  if dataclasses.is_dataclass(value):
    return value.__name__
  raise ValueError(f'unsupported source type: {value}')


def neutral(value):
  origin, args = typing.get_origin(value), typing.get_args(value)
  if origin in (types.UnionType, typing.Union):
    return None
  if origin is typing.Literal:
    return 'unknown' if args in ENUMS and ENUMS[args] == 'GitBranchStatusState' else args[0]
  if origin is tuple:
    return () if args[-1] is Ellipsis else tuple(neutral(argument) for argument in args)
  if dataclasses.is_dataclass(value):
    return default_record(value)
  return {float: 0.0, int: 0, bool: False, str: '', bytes: b'', object: None}[value]


def default_record(cls):
  hints = typing.get_type_hints(cls)
  fields = dataclasses.fields(cls)
  required = {field.name: neutral(hints[field.name]) for field in fields if field.default is dataclasses.MISSING}
  return cls(**required)


def expression(annotation, value) -> str:
  origin, args = typing.get_origin(annotation), typing.get_args(annotation)
  if origin in (types.UnionType, typing.Union):
    if value is None:
      return 'None'
    inner = next(argument for argument in args if argument is not type(None))
    return f'Some({expression(inner, value)})'
  if origin is typing.Literal:
    return ENUMS[args] + '::' + value[:1].upper() + value[1:]
  if annotation is bool:
    return str(value).lower()
  if annotation is str:
    return 'String::new()' if not value else json.dumps(value, ensure_ascii=False) + '.into()'
  if annotation in (int, float):
    return repr(value)
  if annotation is bytes:
    return 'Vec::new()' if not value else 'vec![' + ','.join(map(str, value)) + ']'
  if origin is tuple:
    if len(args) == 2 and args[1] is Ellipsis:
      return 'Vec::new()' if not value else 'vec![' + ','.join(expression(args[0], item) for item in value) + ']'
    return '[' + ','.join(expression(argument, item) for argument, item in zip(args, value, strict=True)) + ']'
  if dataclasses.is_dataclass(value):
    hints = typing.get_type_hints(type(value))
    return (
      type(value).__name__
      + ' {'
      + ','.join(field.name + ':' + expression(hints[field.name], getattr(value, field.name)) for field in dataclasses.fields(value))
      + '}'
    )
  raise ValueError(f'unsupported default expression: {annotation}: {value!r}')
