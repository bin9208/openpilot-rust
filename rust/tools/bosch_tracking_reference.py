#!/usr/bin/env python3
"""Replay every captured tracking transition with the exact original source."""
import copy
import json
import numbers
import sys
from pathlib import Path
from typing import Sequence

import numpy as np

from bosch_acceleration_reference import differences, environment, frozen, source_scope


def original():
  names = {'BOSCH_INACTIVE_WORD', 'BOSCH_MAX_RAW_TRACK_ID', 'bosch_numpy_linear_sum_assignment',
           '_bosch_integer', '_bosch_finite', 'BoschRawDetection', 'BoschRawTrackingConfig',
           'BoschRawTrack', 'BoschRawAssociationDecision', '_BoschRawState', '_bosch_advance',
           '_bosch_admissible_cycle', '_bosch_rectangular_unique', '_bosch_component_matching',
           '_bosch_raw_unique_component', '_bosch_raw_assignment', 'BoschRawTrackManager'}
  scope, hashes = source_scope(names, dict(np=np, Real=numbers.Real, Sequence=Sequence))
  # The captured reference explicitly used this original NumPy solver, not SciPy.
  scope['bosch_linear_sum_assignment'] = scope['bosch_numpy_linear_sum_assignment']
  return scope, hashes


def restore(before, scope):
  before = copy.deepcopy(before)
  manager = scope['BoschRawTrackManager'](scope['BoschRawTrackingConfig'](**before['config']))
  assert vars(manager).keys() == before.keys(), 'original manager fields changed'
  for name, value in before.items():
    if name == 'config':
      continue
    if name == '_states':
      value = {int(key): scope['_BoschRawState'](
        track=scope['BoschRawTrack'](**{**state['track'], 'detection': scope['BoschRawDetection'](**state['track']['detection'])}),
        **{key: item for key, item in state.items() if key != 'track'}) for key, state in value.items()}
    elif name == 'last_decisions':
      value = tuple(scope['BoschRawAssociationDecision'](**item) for item in value)
    elif name == 'last_identity_evidence':
      value = {int(key): tuple(item) for key, item in value.items()}
    setattr(manager, name, value)
  assert not list(differences(before, frozen(manager))), 'original input restoration changed bits'
  return manager


def run(input_path, output_path, evidence_path):
  scope, hashes = original()
  rows = json.loads(Path(input_path).read_text())
  assert len(rows) == 768, 'tracking transition coverage changed'
  changed_rows, different_fields, examples = 0, 0, []
  for index, row in enumerate(rows):
    manager = restore(row['before'], scope)
    historical = {key: row[key] for key in ('result', 'after') if key in row}
    inputs = row['input']
    detections = [scope['BoschRawDetection'](**item) for item in inputs['detections']]
    try:
      result = manager.update(inputs['timestamp_ns'], detections, inputs['yaw_rate'])
    except (ValueError, OverflowError) as error:
      assert row.get('error') == dict(kind=type(error).__name__, message=str(error)), f'error/{index}'
    else:
      assert 'error' not in row, f'original no longer rejects transition/{index}'
      row['result'] = frozen(result)
    row['after'] = frozen(manager)
    count = 0
    for key, value in historical.items():
      for difference in differences(value, row[key], f'{key}/{index}'):
        count += 1
        if len(examples) < 20:
          examples.append(difference)
    changed_rows += bool(count)
    different_fields += count
  Path(output_path).write_text(json.dumps(rows, allow_nan=False, separators=(',', ':')))
  evidence = dict(source_commit='e8e152dbcfd9574a1791ed12026d1880e87689b3', source_files=hashes,
                  **environment(), numpy=np.__version__, transitions=len(rows),
                  frozen_changed_rows=changed_rows, frozen_different_fields=different_fields, examples=examples,
                  comparison='Every result, full state and atomic error; exact floating point bits; original NumPy solver')
  Path(evidence_path).write_text(json.dumps(evidence, indent=2) + '\n')
  print(json.dumps(evidence), file=sys.stderr)


if __name__ == '__main__':
  run(*sys.argv[1:])
