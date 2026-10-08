from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
import json
from pathlib import Path
import time
from typing import Protocol

from carrot_navi_qa.socket import Json
from radarcan_exact import assert_exact


class Queue(Protocol):
  def receive(self, non_blocking: bool = False) -> bytes | None: ...


META_NAMES = {
  'vehicle': 'vehicle', 'guidance_current': 'guidanceCurrent', 'guidance_next': 'guidanceNext',
  'lane_current': 'laneCurrent', 'lane_ahead': 'laneAhead', 'speed': 'speed',
  'traffic_signal': 'trafficSignal', 'crossroad': 'crossroad', 'route': 'route',
  'navigation_status': 'navigationStatus',
}


@dataclass(slots=True)
class Observer:
  output: Path
  state: Queue = field(init=False)
  media: Queue = field(init=False)
  publications: list[dict[str, Json]] = field(default_factory=list)
  selected: list[dict[str, Json]] = field(default_factory=list)
  started_ns: int = field(default_factory=time.monotonic_ns)
  last_state: dict[str, Json] | None = None
  media_payloads: dict[str, bytes] = field(default_factory=dict)
  delivered_media: set[str] = field(default_factory=set)

  def __post_init__(self) -> None:
    from openpilot.cereal import messaging
    self.state = messaging.sub_sock('carrotNavi', conflate=False)
    self.media = messaging.sub_sock('carrotNaviMedia', conflate=False)

  def drain(self) -> list[dict[str, Json]]:
    from openpilot.cereal import log
    states = []
    for topic, queue in (('carrotNaviMedia', self.media), ('carrotNavi', self.state)):
      while (packet := queue.receive(non_blocking=True)) is not None:
        with log.Event.from_bytes(packet) as message:
          decoded = message.to_dict()
        row = {'topic': topic, 'wire_hex': packet.hex(), 'event': decoded,
          'receive_ns': time.monotonic_ns()}
        self.publications.append(row)
        if topic == 'carrotNavi':
          states.append(decoded)
          self.last_state = decoded
    return states

  def register_media(self, key: str, payload: bytes) -> None:
    self.media_payloads[key] = payload

  def media_delivered(self, health: dict[str, Json]) -> bool:
    self.delivered_media.clear()
    items = health['items']
    if not isinstance(items, dict):
      raise TypeError('health items are not an object')
    for row in self.publications:
      if row['topic'] != 'carrotNaviMedia':
        continue
      event = row['event']
      if not isinstance(event, dict):
        raise TypeError('media event is not an object')
      body = event['carrotNaviMedia']
      if not isinstance(body, dict):
        raise TypeError('media payload is not a struct')
      key = f"{body['kind']}:{body['name']}"
      record = items.get(key)
      if not isinstance(record, dict):
        continue
      if body['sequence'] != record['sequence']:
        continue
      for field_name, original in (('sequence', 'sequence'), ('sourceTimestampMillis', 'source_timestamp_ms'),
          ('present', 'present'), ('messageType', 'message_type'), ('formatOrReason', 'format_or_reason'),
          ('flags', 'flags'), ('width', 'width'), ('height', 'height')):
        assert_exact(body[field_name], record[original], f'media/{key}/{field_name}')
      if body['payload'] != self.media_payloads[key]:
        raise ValueError(f'media/{key}/payload bytes mismatch')
      assert_exact(body['reason'], record['reason'] or '', f'media/{key}/reason')
      mono = body['receivedMonoTimeNanos']
      if not isinstance(mono, int) or not self.started_ns <= mono <= time.monotonic_ns():
        raise ValueError('media monotonic timestamp is outside the owned process interval')
      if body['sessionId'] != health['session_id'] or body['schemaVersion'] != 1 or not event['valid']:
        raise ValueError('media session/schema/validity mismatch')
      self.delivered_media.add(key)
    expected = {key for key in items if key.startswith(('image:', 'render:'))}
    return expected <= self.delivered_media

  def check(self, event: dict[str, Json], health: dict[str, Json]) -> None:
    from openpilot.cereal import log
    from openpilot.selfdrive.carrot.carrot_navi_cereal import build_carrot_navi_payload
    body = event['carrotNavi']
    if not isinstance(body, dict):
      raise TypeError('carrotNavi payload is not a struct')
    items = health['items']
    if not isinstance(items, dict):
      raise TypeError('health items are not an object')
    records = {}
    for name, field_name in META_NAMES.items():
      record = items.get('json:' + name)
      if not isinstance(record, dict):
        continue
      field_value = body[field_name]
      if name == 'lane_ahead':
        if not isinstance(field_value, list):
          raise TypeError('laneAhead is not a list')
        metadata = field_value[0]['meta'] if field_value else None
      else:
        metadata = field_value['meta'] if isinstance(field_value, dict) else None
      if metadata is None:
        continue
      if not isinstance(metadata, dict):
        raise TypeError('Cereal metadata is not a struct')
      mono = metadata['receivedMonoTimeNanos']
      if not isinstance(mono, int) or not self.started_ns <= mono <= time.monotonic_ns():
        raise ValueError('record monotonic timestamp is outside the owned process interval')
      records[name] = {'present': record['present'], 'sequence': record['sequence'],
        'source_timestamp_ms': record['source_timestamp_ms'], 'received_mono_ns': mono,
        'value': record['value']}
    snapshot = {'generation': health['state_generation'], 'session_id': health['session_id'] or '',
      'connected': bool(health['control_connected'] and health['session_id']), 'items': records}
    expected = log.Event.new_message(valid=True, logMonoTime=event['logMonoTime'])
    expected.init('carrotNavi')
    expected.carrotNavi = build_carrot_navi_payload(snapshot, publish_mono_ns=body['publishMonoTimeNanos'])
    assert_exact(event, expected.to_dict(), 'actual Cereal complete fields')
    if not event['valid']:
      raise ValueError('Carrot Navi publication must be valid')

  async def observe(self, health: dict[str, Json]) -> None:
    generation = health['state_generation']
    deadline = asyncio.get_running_loop().time() + 5
    while True:
      for event in self.drain():
        body = event['carrotNavi']
        if isinstance(body, dict) and body['generation'] == generation:
          self.check(event, health)
          self.selected.append({'health': health, 'event': event})
          if self.media_delivered(health):
            return
      if self.last_state is not None:
        body = self.last_state['carrotNavi']
        if isinstance(body, dict) and body['generation'] == generation and self.media_delivered(health):
          self.check(self.last_state, health)
          self.selected.append({'health': health, 'event': self.last_state})
          return
      if asyncio.get_running_loop().time() >= deadline:
        raise TimeoutError(f'Cereal generation {generation} did not arrive')
      await asyncio.sleep(.001)

  def save(self) -> None:
    self.drain()
    self.output.joinpath('publications.json').write_text(json.dumps(self.publications, ensure_ascii=True, indent=2, default=binary_json))
    self.output.joinpath('selected-publications.json').write_text(json.dumps(self.selected, ensure_ascii=True, indent=2))


def binary_json(value: object) -> Json:
  if isinstance(value, bytes):
    return {'binary_hex': value.hex()}
  raise TypeError('unexpected Cereal value in JSON capture')
