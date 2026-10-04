from __future__ import annotations

import pytest

from radarcan_exact import assert_exact


def test_signed_zero_is_an_observable_float_mismatch() -> None:
  with pytest.raises(AssertionError):
    assert_exact({'point': [-0.0]}, {'point': [0.0]})


def test_boolean_is_not_an_integer_flag() -> None:
  with pytest.raises(AssertionError):
    assert_exact({'canError': 1}, {'canError': True})


def test_rounded_json_float_cannot_replace_an_exact_timestamp() -> None:
  with pytest.raises(AssertionError):
    assert_exact(float(2**53), 2**53 + 1)


def test_boolean_cannot_replace_an_integer_packet_count() -> None:
  with pytest.raises(AssertionError):
    assert_exact(False, 0)


def test_source_integer_frequency_and_native_float_frequency_are_equal() -> None:
  assert_exact({'frequency': 20.0, 'counter': 1}, {'frequency': 20, 'counter': 1})
