"""Unchanged onboarding classes with an owned VisionIPC resource name."""

import ast
import math
from types import SimpleNamespace
from compact_source import classes


def load(root, gui_app, ui_state, device):
  import openpilot.selfdrive.ui.ui_state as state

  state.UIStatus = SimpleNamespace(DISENGAGED=0, ENGAGED=1, OVERRIDE=2)
  from openpilot.selfdrive.ui.mici.onroad.cameraview import CameraView
  from openpilot.selfdrive.ui.mici.onroad.driver_state import DriverStateRenderer
  from openpilot.system.ui.widgets.nav_widget import NavWidget
  from openpilot.system.ui.widgets.button import SmallCircleIconButton
  from openpilot.common.filter_simple import FirstOrderFilter
  from openpilot.system.version import terms_version, training_version
  from openpilot.system.ui.lib.application import TextAlignmentVertical
  from msgq.visionipc import VisionStreamType
  from openpilot.cereal import log, messaging

  class OwnedCamera(CameraView):
    def __init__(self, name, stream):
      super().__init__('rustvision', stream)

  namespace = vars(classes(root, gui_app, ui_state))
  namespace.update(
    CameraView=OwnedCamera,
    DriverStateRenderer=DriverStateRenderer,
    NavWidget=NavWidget,
    SmallCircleIconButton=SmallCircleIconButton,
    FirstOrderFilter=FirstOrderFilter,
    terms_version=terms_version,
    training_version=training_version,
    device=device,
    TextAlignmentVertical=TextAlignmentVertical,
    VisionStreamType=VisionStreamType,
    math=math,
    log=log,
    messaging=messaging,
  )
  for path, names in [
    ('openpilot/selfdrive/ui/mici/onroad/driver_camera_dialog.py', ['DriverCameraView', 'BaseDriverCameraDialog']),
    ('openpilot/selfdrive/ui/mici/layouts/onboarding.py', ['DriverCameraSetupDialog', 'TrainingGuideDMTutorial', 'TrainingGuide', 'OnboardingWindow']),
  ]:
    tree = ast.parse((root / path).read_text())
    nodes = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in names]
    assert len(nodes) == len(names)
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
  return SimpleNamespace(**namespace)
