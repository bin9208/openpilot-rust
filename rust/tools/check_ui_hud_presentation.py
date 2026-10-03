import argparse
import ast
from dataclasses import dataclass
import importlib.util
import json
from pathlib import Path
import random
import struct
import subprocess
from types import SimpleNamespace


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  root = Path(__file__).resolve().parents[2]
  path = root / 'openpilot/selfdrive/carrot/deceleration_source.py'
  spec = importlib.util.spec_from_file_location('original_presentation', path)
  assert spec is not None and spec.loader is not None
  presentation = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(presentation)
  overrides = []
  current_label = ['MAX']
  for relative in ['onroad/hud_renderer.py', 'mici/onroad/hud_renderer.py']:
    path = root / 'openpilot/selfdrive/ui' / relative
    tree = ast.parse(path.read_text())
    nodes = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in ['SetSpeedOverrideState', 'SetSpeedOverride']]
    namespace = {'dataclass': dataclass, 'tr': lambda _: current_label[0],
                 'deceleration_source_presentation': presentation.deceleration_source_presentation}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
    overrides.append(namespace['SetSpeedOverride']())
  sources = ['cam', 'section', 'bump', 'police', 'waze', 'road', 'atc', 'atc2', 'route', 'hda', 'hda_section',
             'hda_bump', 'school', 'gas', 'vturn', 'model', 'turn', '', 'unknownsource', '카메라구간속도위험경고',
             'ΟΣ', '\u001c CAM\u001f', '\u2003HDA\u2009']
  reasons = [source + suffix for source in sources for suffix in ['', ':n', ':v', ':c']]
  providers = ['hda', 'naver_v1', 'tmap_legacy', '', 'unknown', 'HDA', ' hda']
  rng = random.Random(14814)
  steps = []
  source_results = []
  for index in range(6000):
    value = rng.choice(['0', '-1', '255', '200', 'NaN', 'inf', '-inf', '-0.0', '30', '60', '100', '199.99'])
    target = rng.choice([None, value, '0', '100', '100.5', '100.50000000000001', 'NaN', 'inf', '-inf'])
    desired = rng.choice([None, value, '0', '199.99999999999997', '200', 'NaN', 'inf', '-inf'])
    step = {'source': reasons[index % len(reasons)], 'provider': rng.choice(providers), 'target': target,
            'desired': desired, 'set_speed': value, 'max_label': rng.choice(['MAX', '최대']),
            'vehicle': bool(rng.randrange(2)), 'external': bool(rng.randrange(2)),
            'owner': rng.choice(['', 'naver_v1', 'tmap_legacy', 'unknown', 'NAVER_V1']),
            'lifecycle': rng.choice(['', 'guiding', 'idle', 'stopped', 'arrived', 'GUIDING']),
            'remote': rng.choice(['', '  ', '\u001c\u001f', '\u2003', 'naver', '\u2003 a \u001f']),
            'connected': bool(rng.randrange(2))}
    steps.append(step)
    current_label[0] = step['max_label']
    messages = {'longitudinalPlan': SimpleNamespace(), 'carrotMan': SimpleNamespace(desiredSource=step['source'], decelProvider=step['provider'])}
    if target is not None:
      messages['longitudinalPlan'].cruiseTarget = float(target)
    if desired is not None:
      messages['carrotMan'].desiredSpeed = float(desired)
    values = [override.compute(messages, float(value)) for override in overrides]
    observed = []
    for output in values:
      observed.append({'active': output.active, 'speed_bits': str(struct.unpack('<Q', struct.pack('<d', output.speed_kph))[0]),
                       'label': output.label, 'speed_color_mode': output.speed_color_mode, 'force_persist': output.force_persist})
    assert observed[0] == observed[1], (index, observed)
    source_results.append({'override': observed[0], 'reason': presentation.deceleration_source_presentation(step['source'], step['provider']),
                           'navigation': presentation.navigation_status_presentation(step['vehicle'], step['external'], step['owner'], step['lifecycle']),
                           'connected': presentation.external_navigation_connected(step['remote'], step['connected'])})
  serialized = json.dumps(steps)
  (args.output / 'input.json').write_text(serialized)
  source = json.loads(json.dumps(source_results))
  (args.output / 'source.json').write_text(json.dumps(source))
  process = subprocess.run([str(args.binary)], input=serialized, capture_output=True, text=True, check=True)
  (args.output / 'native.json').write_text(process.stdout)
  (args.output / 'native.stderr').write_text(process.stderr or '(no stderr)\n')
  native = json.loads(process.stdout)
  differences = [{'case': index, 'source': a, 'native': b} for index, (a, b) in enumerate(zip(source, native, strict=True)) if a != b]
  (args.output / 'result.json').write_text(json.dumps({'cases': len(steps), 'differences': differences}, indent=2))
  assert not differences, differences[:1]
  print(f'PASS {len(steps)} original/native big+compact HUD overrides and display-only source/navigation presentation cases')


if __name__ == '__main__':
  main()
