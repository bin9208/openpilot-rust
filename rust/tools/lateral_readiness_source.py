"""Hash-verified final-source lateral gate and scoped consumer AST fragments (#338)."""
import ast
import copy
import hashlib
import json
from pathlib import Path
import sys
from types import ModuleType

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / 'rust/upstream/lateral-readiness-source.json'


def source(path):
    fixture = json.loads(FIXTURE.read_text())
    assert fixture['source_repo'] == 'https://github.com/ajouatom/openpilot'
    assert fixture['source_commit'] == 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
    record = fixture['files'][path]
    data = record['content'].encode()
    assert hashlib.sha256(data).hexdigest() == record['sha256']
    assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == record['git_blob']
    return record['content']


def module():
    path = 'openpilot/selfdrive/controls/lib/lateral_readiness.py'
    result = ModuleType('verified_lateral_readiness')
    sys.modules[result.__name__] = result
    exec(compile(source(path), f'e8e152db:{path}', 'exec'), result.__dict__)
    return result


def method(tree, owner, name):
    cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == owner)
    return next(node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name == name)


def assignment(body, target):
    return next(node for node in body if isinstance(node, ast.Assign)
                and any(ast.unparse(value) == target for value in node.targets))


def patch_controlsd(tree, scope):
    """Only replace startup-gate fragments; unrelated final DM/impact ports stay separate."""
    verified = module()
    scope.update(LateralStartupGate=verified.LateralStartupGate,
                 lateral_vehicle_parameters=verified.lateral_vehicle_parameters)
    ratio = ModuleType('verified_steer_ratio')
    exec(compile(source('openpilot/selfdrive/controls/lib/steer_ratio.py'), 'e8e152db:steer_ratio', 'exec'), ratio.__dict__)
    scope['resolve_vehicle_model_steer_ratio'] = ratio.resolve_vehicle_model_steer_ratio
    final = ast.parse(source('openpilot/selfdrive/controls/controlsd.py'))
    init = method(tree, 'Controls', '__init__')
    final_init = method(final, 'Controls', '__init__')
    for name in ['self.lateral_startup', 'self.lateral_started']:
        init.body.append(copy.deepcopy(assignment(final_init.body, name)))
    target = method(tree, 'Controls', 'state_control')
    final_control = method(final, 'Controls', 'state_control')
    for name in ['lp', 'sr']:
        old = assignment(target.body, name)
        target.body[target.body.index(old)] = copy.deepcopy(assignment(final_control.body, name))
    after_car = target.body.index(assignment(target.body, 'CS')) + 1
    target.body.insert(after_car, copy.deepcopy(assignment(final_control.body, 'lateral_ready')))
    suspension = next(node for node in target.body if isinstance(node, ast.Assign)
                      and ast.unparse(node.value).startswith('self.carrot_controls.lat_suspend_control'))
    gate = next(node for node in final_control.body if isinstance(node, ast.Assign)
                and ast.unparse(node) == 'CC.latActive = CC.latActive and lateral_ready')
    target.body.insert(target.body.index(suspension)+1, copy.deepcopy(gate))
    startup = next(node for node in final_control.body if isinstance(node, ast.If)
                   and ast.unparse(node.test) == 'CC.latActive and (not self.lateral_started)')
    reset = next(node for node in target.body if isinstance(node, ast.If) and ast.unparse(node.test) == 'not CC.latActive')
    target.body.insert(target.body.index(reset)+1, copy.deepcopy(startup))
    # Adjacent approved DM producer consumer: use final published-disable semantics.
    publish = method(tree, 'Controls', 'publish')
    final_publish = method(final, 'Controls', 'publish')
    force = assignment(final_publish.body, 'cs.forceDecel')
    indices = [i for i,node in enumerate(publish.body)
               if (isinstance(node,ast.Assign) and any(ast.unparse(t)=='cs.forceDecel' for t in node.targets))
               or (isinstance(node,ast.If) and any(isinstance(child,ast.Assign)
                   and any(ast.unparse(t)=='cs.forceDecel' for t in child.targets) for child in ast.walk(node)))]
    assert len(indices)==2
    publish.body[indices[0]:indices[-1]+1]=[copy.deepcopy(force)]


def patch_card(node):
    final = ast.parse(source('openpilot/selfdrive/car/card.py'))
    target = next(item for item in node.body if isinstance(item, ast.FunctionDef) and item.name == 'controls_update')
    final_control = method(final, 'Car', 'controls_update')
    def apply_body(fn):
        return next(item.body for item in fn.body if isinstance(item, ast.If)
                    and ast.unparse(item.test) == "self.sm.all_alive(['carControl'])")
    body, final_body = apply_body(target), apply_body(final_control)
    ready = assignment(final_body, 'lateral_ready')
    guard = next(item for item in final_body if isinstance(item, ast.If)
                 and ast.unparse(item.test) == 'CC.latActive and (not lateral_ready)')
    index = body.index(assignment(body, 'apply_start_ns'))
    body[index:index] = [copy.deepcopy(ready), copy.deepcopy(guard)]
