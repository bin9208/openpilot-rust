"""Run unchanged source passive fingerprint and MAIN hold logic with explicit boundary inputs."""

import types
import ast

from can_source import ROOT, load


def fingerprint(batches):
  load()
  from opendbc.car.can_definitions import CanData, CanRecvCallable
  from opendbc.car import gen_empty_fingerprint
  from opendbc.car.fingerprints import all_legacy_fingerprint_cars, eliminate_incompatible_cars
  path = ROOT / 'opendbc_repo/opendbc/car/car_helpers.py'
  tree = ast.parse(path.read_text())
  # Run the entire unchanged function; omit unrelated active-query imports (tqdm/firmware dependencies).
  nodes = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'can_fingerprint']
  nodes += [node for node in tree.body if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'FRAME_FINGERPRINT' for target in node.targets)]
  scope = dict(gen_empty_fingerprint=gen_empty_fingerprint, all_legacy_fingerprint_cars=all_legacy_fingerprint_cars,
               eliminate_incompatible_cars=eliminate_incompatible_cars, CanRecvCallable=CanRecvCallable)
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), scope)
  iterator = iter(batches)
  received = 0
  def receive(wait_for_one=False):
    nonlocal received
    batch = next(iterator)
    received += len(batch)
    return [[CanData(frame['address'], bytes(frame['data']), frame['bus']) for frame in packet['frames']] for packet in batch]
  selected, observed = scope['can_fingerprint'](receive)
  return dict(selected=selected, observed=[[bus, [[address, length] for address, length in signals.items()]] for bus, signals in observed.items()], frames=received)


def toggle(steps):
  from openpilot.selfdrive.car.openpilot_toggle import CruiseMainOpenpilotToggle
  state = CruiseMainOpenpilotToggle(8)
  outputs = []
  for step in steps:
    buttons = [types.SimpleNamespace(type=button['kind'], pressed=button['pressed']) for button in step['buttons']]
    fired = state.update(buttons, step['engaged'], step['now'])
    outputs.append(dict(fired=fired, pressed_at=state._pressed_at, triggered=state._triggered))
  return outputs
