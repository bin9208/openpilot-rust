# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Synthetic full-cereal controls for carrot_server_live.py; caller provides original pycapnp dependencies.
from __future__ import annotations
import base64
import hashlib
from pathlib import Path
from typing import TypeAlias, TypedDict
from openpilot.cereal import log
from openpilot.selfdrive.carrot.realtime.compact_state import CARROT_STATE_SERVICES, encode_carrot_state_frame
from carrot_server_dashcam_upload import save


FieldValue: TypeAlias = None | bool | int | float | str | bytes | list["FieldValue"] | dict[str, "FieldValue"]


class Frame(TypedDict):
  service: str
  path: str
  expected: str


def frames(output: Path) -> list[Frame]:
  output.mkdir()
  values = {
    'carState': {'vEgo': 19.25, 'aEgo': float('nan'), 'gearShifter': 'drive', 'evModeValid': True, 'tpms': {'fl': 36.5}},
    'selfdriveState': {'enabled': True, 'alertText1': 'owned 한글', 'alertStatus': 'critical', 'personality': 'moreRelaxed'},
    'deviceState': {'started': True, 'freeSpacePercent': float('inf'), 'cpuTempC': [44.5, float('nan')], 'deviceType': 'tizi'},
    'modelV2': {
      'frameId': 34,
      'position': {'x': [0.005, 655.4, 1000.0], 'y': [-0.0005, -40.0, 50.0], 'z': [0.01]},
      'leadsV3': [{'prob': 0.8, 'x': [120.0, 150.0], 'y': [-2.0], 'v': [3.0]}],
    },
    'longitudinalPlan': {'accels': [-1.25, 2.0], 'speeds': [11.0, 12.0], 'jerks': [], 'cruiseTarget': 22.5},
    'liveTracks': {'points': [{'trackId': 71, 'dRel': 22.0, 'yRel': -4.0, 'radarSource': 'corner235', 'measured': True}]},
    'livePose': {'inputsOK': True, 'orientationNED': {'x': 0.1, 'valid': True, 'xStd': 0.02}, 'timestamp': 1_000_000_001},
    'carrotNavi': {
      'schemaVersion': 1,
      'generation': 20,
      'sessionId': 'owned-navigation',
      'route': {
        'polyline': [{'latitude': 37.1, 'longitude': 127.1}, {'latitude': 37.10001, 'longitude': 127.10002}, {'latitude': float('nan'), 'longitude': 0.0}]
      },
      'navigationStatus': {'guidanceActive': True},
    },
  }
  result = []
  for name in CARROT_STATE_SERVICES:
    message = log.Event.new_message()
    message.valid = True
    message.logMonoTime = 1_000_000_000
    message.init(name)
    message.from_dict({name: values.get(name, {})})
    raw = message.to_bytes()
    path = output / (name + '.capnp')
    path.write_bytes(raw)
    with log.Event.from_bytes(raw) as parsed:
      encoded = encode_carrot_state_frame(name, getattr(parsed, name), 65535)
    result.append({'service': name, 'path': str(path), 'expected': base64.b64encode(encoded).decode()})
  save(
    output / 'compact-source.json',
    {'frames': result, 'source_sha256': hashlib.sha256(Path('openpilot/selfdrive/carrot/realtime/compact_state.py').read_bytes()).hexdigest()},
  )
  return result


def event(output: Path, name: str, values: dict[str, FieldValue], tag: str) -> Frame:
  message = log.Event.new_message()
  message.valid = True
  message.logMonoTime = 1_000_000_000
  message.init(name)
  message.from_dict({name: values})
  path = output / (tag + '.capnp')
  path.write_bytes(message.to_bytes())
  return {'service': name, 'path': str(path), 'expected': ''}
