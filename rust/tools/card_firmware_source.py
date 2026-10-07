# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Imported by check_card_firmware.py; progress display is the only replaced source dependency.
import sys
import types
from typing import TypedDict, Literal
from can_source import load


class Firmware(TypedDict):
  ecu: str
  address: int
  sub_address: int
  fw_version: list[int]
  brand: str
  logging: bool


class Case(TypedDict, total=False):
  op: Literal['match', 'select', 'codes']
  versions: list[Firmware] | list[list[int]]
  vin: str
  exact: bool
  fuzzy: bool
  brand: str
  name: str


def setup() -> None:
  load()
  progress = types.ModuleType('tqdm')
  progress.tqdm = lambda iterable, disable=True: iterable
  sys.modules['tqdm'] = progress


def trace(case: Case):
  from opendbc.car import structs, fw_versions, selected_car
  match case['op']:
    case 'match':
      versions = []
      for entry in case['versions']:
        version = structs.CarParams.CarFw()
        version.ecu = entry['ecu']
        version.address = entry['address']
        version.subAddress = entry['sub_address']
        version.fwVersion = bytes(entry['fw_version'])
        version.brand = entry['brand']
        version.logging = entry['logging']
        versions.append(version)
      exact, matches = fw_versions.match_fw_to_car(versions, case['vin'], case['exact'], case['fuzzy'], log=False)
      return dict(exact=exact, candidates=sorted(matches))
    case 'select':
      result = selected_car.get_selected_car_platform(case['name'])
      return str(result) if result is not None else None
    case 'codes':
      module = __import__(f'opendbc.car.{case["brand"]}.values', fromlist=['get_platform_codes'])
      codes = module.get_platform_codes([bytes(version) for version in case['versions']])
      if case['brand'] == 'toyota':
        return [[list(key), None] for key in sorted(codes)]
      return [[list(key), list(date) if date is not None else None] for key, date in sorted(codes)]
    case unreachable:
      from typing import assert_never
      assert_never(unreachable)
