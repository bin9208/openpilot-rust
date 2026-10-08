from __future__ import annotations

import struct
from typing import TypeAlias, assert_never

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']


def assert_exact(actual: Json, expected: Json, path: str = '$') -> None:
  match expected:
    case bool():
      assert type(actual) is bool and actual == expected, (path, actual, expected)
    case float():
      assert type(actual) in (int, float), (path, actual, expected)
      assert struct.pack('<d', actual) == struct.pack('<d', expected), (path, actual, expected)
    case dict():
      assert isinstance(actual, dict) and actual.keys() == expected.keys(), (path, actual, expected)
      for key, value in expected.items():
        assert_exact(actual[key], value, path + '/' + str(key))
    case list():
      assert isinstance(actual, list) and len(actual) == len(expected), (path, actual, expected)
      for index, value in enumerate(expected):
        assert_exact(actual[index], value, path + '/' + str(index))
    case int():
      assert type(actual) in (int, float) and actual == expected, (path, actual, expected)
    case str() | None:
      assert actual == expected, (path, actual, expected)
    case unreachable:
      assert_never(unreachable)
