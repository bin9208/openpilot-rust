# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Imported by the model daemon QA scripts to execute actual source log expressions.
import ast
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
import re
from types import SimpleNamespace


class Source:
  def __init__(self, component: str):
    self.path = Path(__file__).resolve().parents[2] / f'openpilot/selfdrive/modeld/{component}.py'
    self.text = self.path.read_text()
    self.calls = sorted((node for node in ast.walk(ast.parse(self.text)) if isinstance(node, ast.Call)
                        and isinstance(node.func, ast.Attribute) and isinstance(node.func.value, ast.Name)
                        and node.func.value.id == 'cloudlog'), key=lambda node: node.lineno)
    self.used = set()

  def expected(self, prefix: str, scope: dict | None = None) -> tuple[int, str]:
    def first(node):
      match node:
        case ast.Constant(value=str() as text):
          return text
        case ast.JoinedStr(values=[ast.Constant(value=str() as text), *_]):
          return text
        case _:
          raise AssertionError(ast.dump(node))
    matches = [call for call in self.calls if first(call.args[0]).startswith(prefix)]
    assert len(matches) == 1, (prefix, matches)
    call = matches[0]
    self.used.add(call.lineno)
    result = []
    level = {'warning': 30, 'info': 20, 'error': 40, 'debug': 10, 'exception': 40}[call.func.attr]

    def emit(message, *arguments):
      result.append((level, message % arguments if arguments else message))

    namespace = dict(scope or {}, cloudlog=SimpleNamespace(**{call.func.attr: emit}))
    code = compile(ast.fix_missing_locations(ast.Module(body=[ast.Expr(value=call)], type_ignores=[])), str(self.path), 'exec')
    exec(code, namespace)
    assert len(result) == 1
    return result[0]

  def inventory(self) -> dict:
    return {'compared_lines': sorted(self.used), 'pending_egpu_lines': [
      {'line': call.lineno, 'expression': ast.get_source_segment(self.text, call)}
      for call in self.calls if call.lineno not in self.used]}


@dataclass(frozen=True, slots=True)
class DrivingScenario:
  pid: int
  resolution: tuple[int, int]
  size: int
  mode: str
  frame_ids: tuple[int, ...]


def driving(records: list[dict], scenario: DrivingScenario) -> dict:
  source = Source('modeld')
  actual = [(record['levelnum'], record['msg']) for record in records
            if record['process'] == scenario.pid and isinstance(record['msg'], str)]
  camera = SimpleNamespace(width=scenario.resolution[0], height=scenario.resolution[1], buffer_len=scenario.size)
  scope = {'use_wide_camera': True, 'main_wide_camera': scenario.mode == 'wide', 'use_extra_client': scenario.mode == 'dual',
           'vipc_client_main': camera, 'vipc_client_extra': camera, 'CP': SimpleNamespace(brand='')}
  expected = [source.expected(prefix, scope) for prefix in ['modeld init', 'vision stream set up', 'connected main cam',
                                                          'loading model', 'modeld got CarParams']]
  extra_record = source.expected('connected extra cam', scope)
  if scenario.mode == 'dual':
    expected.append(extra_record)
  loaded = [message for _level, message in actual if message.startswith('models loaded in ')]
  assert len(loaded) == 1
  duration = re.fullmatch(r'models loaded in ([0-9]+\.[0-9])s, modeld starting', loaded[0])
  assert duration is not None
  expected.append(source.expected('models loaded in ', {'st': 0.0, 'time': SimpleNamespace(monotonic=lambda: float(duration[1]))}))
  loading_record = next(record for record in records if record['process'] == scenario.pid and record['msg'] == 'loading model')
  loaded_record = next(record for record in records if record['process'] == scenario.pid and record['msg'] == loaded[0])
  assert abs(float(duration[1]) - (loaded_record['created'] - loading_record['created'])) <= .2
  previous = 0
  for frame in scenario.frame_ids:
    dropped = max(frame - previous - 1, 0)
    if dropped:
      expected.append(source.expected('camera dropped ', {'vipc_dropped_frames': dropped}))
    previous = frame
  debug = source.expected('camera pair unavailable')
  expected.extend([debug] * actual.count(debug))
  assert actual[0] == source.expected('modeld init')
  assert Counter(actual) == Counter(expected), (actual, expected)
  interrupted = source.expected('got SIGINT')
  assert sum((record['levelnum'], record['msg']) == interrupted for record in records) == 1
  result = source.inventory()
  startup = [message for _level, message in actual if message.startswith(('connected main cam', 'connected extra cam',
             'loading model', 'models loaded in', 'modeld got CarParams'))]
  ordered = ['connected main cam'] + (['connected extra cam'] if scenario.mode == 'dual' else [])
  ordered += ['loading model', 'models loaded in', 'modeld got CarParams']
  assert len(startup) == len(ordered) and all(text.startswith(prefix) for text, prefix in zip(startup, ordered, strict=True))
  result.update(result='pass', compared_records=len(actual), startup_order='model load before CarParams')
  return result


def driver(records: list[dict], scenario: tuple[int, int]) -> dict:
  pid, size = scenario
  source = Source('dmonitoringmodeld')
  expected = [source.expected('connecting to driver stream'),
              source.expected('connected with buffer size', {'vipc_client': SimpleNamespace(buffer_len=size)}),
              source.expected('models loaded, dmonitoringmodeld starting')]
  actual = [(record['levelnum'], record['msg']) for record in records if record['process'] == pid]
  assert actual == expected, (actual, expected)
  other = [(record['levelnum'], record['msg']) for record in records if record['process'] != pid]
  assert other == [source.expected('connecting to driver stream'), source.expected('got SIGINT')]
  result = source.inventory()
  assert not result['pending_egpu_lines']
  result.update(result='pass', compared_records=len(actual) + len(other))
  return result
