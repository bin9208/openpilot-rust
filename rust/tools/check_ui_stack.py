"""Compare actual GuiApplication stack methods without constructing a GL window."""

import argparse
import ast
import hashlib
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
from collections.abc import Callable

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
path = root / 'openpilot/system/ui/lib/application.py'
text = path.read_text()
tree = ast.parse(text)
names = {'push_widget', 'pop_widget', 'pop_widgets_to', 'get_active_widget', 'widget_in_stack'}
source = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'GuiApplication')
methods = [node for node in source.body if isinstance(node, ast.FunctionDef) and node.name in names]
assert len(methods) == len(names)
module = ast.Module(body=[ast.ClassDef(name='SourceStack', bases=[], keywords=[], body=methods, decorator_list=[])], type_ignores=[])
namespace = {'Callable': Callable, 'cloudlog': SimpleNamespace(warning=lambda message: None)}
exec(compile(ast.fix_missing_locations(module), str(path), 'exec'), namespace)
app = namespace['SourceStack']()
app._nav_stack = []
events = []


class Probe:
  def __init__(self, index):
    self.index = index
    self.enabled = True

  def set_enabled(self, value):
    self.enabled = value

  def show_event(self):
    events.append(f'show:{self.index}')

  def hide_event(self):
    events.append(f'hide:{self.index}')

  def dismiss(self, callback):
    events.append(f'dismiss:{self.index}')
    app.pop_widget()
    if callback:
      callback()


widgets = [Probe(index) for index in range(4)]
operations = [
  {'operation': 'pop', 'index': None},
  {'operation': 'push', 'id': 0},
  {'operation': 'push', 'id': 1},
  {'operation': 'push', 'id': 1},
  {'operation': 'push', 'id': 2},
  {'operation': 'pop', 'index': 0},
  {'operation': 'pop', 'index': 8},
  {'operation': 'pop', 'index': 1},
  {'operation': 'pop_to', 'id': 3, 'instant': False},
  {'operation': 'pop_to', 'id': 2, 'instant': False},
  {'operation': 'push', 'id': 1},
  {'operation': 'push', 'id': 3},
  {'operation': 'pop_to', 'id': 0, 'instant': False},
  {'operation': 'pop', 'index': None},
  {'operation': 'push', 'id': 1},
  {'operation': 'push', 'id': 2},
  {'operation': 'push', 'id': 3},
  {'operation': 'pop_to', 'id': 1, 'instant': True},
  {'operation': 'pop', 'index': None},
]
expected = []
for operation in operations:
  match operation['operation']:
    case 'push':
      app.push_widget(widgets[operation['id']])
    case 'pop':
      app.pop_widget(operation['index'])
    case 'pop_to':
      app.pop_widgets_to(widgets[operation['id']], lambda: events.append('callback'), operation['instant'])
  active = app.get_active_widget()
  expected.append(
    {
      'active': active.index if active else None,
      'enabled': [widget.enabled for widget in widgets],
      'contains': [app.widget_in_stack(widget) for widget in widgets],
      'events': list(events),
      'len': len(app._nav_stack),
    }
  )
(args.output / 'input.json').write_text(json.dumps(operations, indent=2))
(args.output / 'source.json').write_text(json.dumps(expected, indent=2))
result = subprocess.run([str(args.binary)], input=json.dumps(operations), text=True, capture_output=True, check=True)
(args.output / 'native.json').write_text(result.stdout)
(args.output / 'native.stderr').write_text(result.stderr or '(no stderr)\n')
assert json.loads(result.stdout) == expected
(args.output / 'result.json').write_text(
  json.dumps({'operations': len(operations), 'exact': True, 'source_sha256': hashlib.sha256(text.encode()).hexdigest()}, indent=2)
)
print(f'PASS: {len(operations)} actual-source/native stack lifecycle, duplicate/index rejection and callback operations')
