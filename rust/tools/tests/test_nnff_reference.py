"""Execute only the retained NNFF oracle policy without device/native imports."""
import ast
from pathlib import Path
from types import SimpleNamespace
import unittest

SOURCE = Path(__file__).resolve().parents[3] / "openpilot/selfdrive/selfdrived/selfdrived.py"
EMPTY = object()


def notice_policy():
  tree = ast.parse(SOURCE.read_text())
  cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "SelfdriveD")
  method = next((node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name == "update_nnff_alert"), None)
  if method is None:
    # Before the port, execute the actual inline notice branch as the oracle.
    update = next(node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name == "update_events")
    branch = next(node for node in update.body if isinstance(node, ast.If) and "NNFFModelName" in ast.unparse(node.test))
    method = ast.parse("def update_nnff_alert(self, CS): pass").body[0]
    method.body = [branch]
  namespace = {"ET": SimpleNamespace(NO_ENTRY="noEntry"), "EventName": SimpleNamespace(torqueNNLoad="torqueNNLoad"), "EmptyAlert": EMPTY}
  exec(compile(ast.fix_missing_locations(ast.Module(body=[method], type_ignores=[])), str(SOURCE), "exec"), namespace)
  def invoke(controller, car):
    namespace["Params"] = lambda: controller.params
    return namespace["update_nnff_alert"](controller, car)
  return invoke


class Params:
  def __init__(self):
    self.name = b"test"
    self.reads = 0
  def get(self, key):
    assert key == "NNFFModelName"
    self.reads += 1
    return self.name


class Events:
  def __init__(self):
    self.names = []
    self.no_entry = False
  def contains(self, category):
    assert category == "noEntry"
    return self.no_entry
  def add(self, name):
    self.names.append(name)


class Messages(dict):
  frame = 550
  healthy = True
  def all_checks(self):
    return self.healthy


class NoticeReference(unittest.TestCase):
  def fixture(self):
    messages = Messages(deviceState=SimpleNamespace(started=True))
    model = Params()
    events = Events()
    controller = SimpleNamespace(nnff_alerted=False, initialized=True, CP=SimpleNamespace(passive=False), sm=messages,
                                 params=model, events=events, AM=SimpleNamespace(current_alert=EMPTY))
    controller.update_alerts = lambda _: setattr(controller.AM, "current_alert", "nnff")
    car = SimpleNamespace(canValid=True, canTimeout=False)
    return controller, car

  def test_late_readiness_and_late_model_name_are_retried_once(self):
    policy = notice_policy()
    controller, car = self.fixture()
    controller.initialized = False
    policy(controller, car)
    self.assertEqual(controller.events.names, [])
    controller.initialized = True
    controller.sm.frame = 600
    controller.params.name = None
    policy(controller, car)
    self.assertEqual(controller.events.names, [])
    controller.sm.frame = 650
    controller.params.name = b""
    policy(controller, car)
    self.assertEqual(controller.events.names, ["torqueNNLoad"])
    self.assertTrue(controller.nnff_alerted)
    controller.AM.current_alert = EMPTY
    controller.sm.frame = 700
    policy(controller, car)
    self.assertEqual(controller.events.names, ["torqueNNLoad"])
    self.assertEqual(controller.params.reads, 2)

  def test_eligibility_guards_skip_parameter_poll(self):
    policy = notice_policy()
    for gate in ("frame", "passive", "offroad", "can", "timeout", "health", "no_entry", "alert"):
      with self.subTest(gate=gate):
        controller, car = self.fixture()
        if gate == "frame": controller.sm.frame = 551
        elif gate == "passive": controller.CP.passive = True
        elif gate == "offroad": controller.sm["deviceState"].started = False
        elif gate == "can": car.canValid = False
        elif gate == "timeout": car.canTimeout = True
        elif gate == "health": controller.sm.healthy = False
        elif gate == "no_entry": controller.events.no_entry = True
        elif gate == "alert": controller.AM.current_alert = object()
        policy(controller, car)
        self.assertEqual(controller.events.names, [])
        self.assertEqual(controller.params.reads, 0)


if __name__ == "__main__":
  unittest.main()
